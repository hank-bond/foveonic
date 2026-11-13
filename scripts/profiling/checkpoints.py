"""Analyze graph checkpoint storage and measured GPU ancestry without changing retention."""

from collections import defaultdict
import math


def allocation_key(resource):
    """An allocation generation, not a graph output or a shared-reference count."""
    allocation = resource.get('allocation_id')
    generation = resource.get('generation')
    size = resource.get('byte_len')
    if (type(allocation) is not int or type(generation) is not int
            or type(size) is not int or size < 0):
        raise ValueError('invalid GPU allocation identity or size')
    return allocation, generation


def unique_allocations(resources):
    """Count each observed physical allocation once within this snapshot."""
    sizes = {}
    for resource in resources:
        if resource is None:
            continue
        key = allocation_key(resource)[0]
        size = resource['byte_len']
        if key in sizes and sizes[key] != size:
            raise ValueError('conflicting sizes for one GPU allocation')
        sizes[key] = size
    return {'unique_bytes':sum(sizes.values()), 'allocation_count':len(sizes)}


def cache_inventory(snapshot):
    entries = snapshot['entries']
    groups = defaultdict(list)
    for entry in entries:
        groups[allocation_key(entry)[0]].append(entry)
    allocations = []
    for allocation, aliases in groups.items():
        size = unique_allocations(aliases)['unique_bytes']
        allocations.append({'allocation_id':allocation, 'byte_len':size,
                            'aliases':[{'version':entry['version'], 'generation':entry['generation'], 'role':entry['role'],
                                        'names':entry['names'],
                                        **{field:entry[field] for field in
                                           ('selected','primary','restart','pinned','required_cut','priority','used_frame')}}
                                       for entry in aliases]})
    flagged = {flag:unique_allocations([entry for entry in entries if entry[flag]])['unique_bytes']
               for flag in ('selected','primary','restart','pinned','required_cut')}
    flagged['unselected_versions'] = unique_allocations([entry for entry in entries if not entry['selected']])['unique_bytes']
    return {'gpu':unique_allocations(entries), 'allocations':allocations,
            'versions':snapshot.get('versions', []),
            'gpu_bytes_by_flag_nonadditive':flagged,
            'cpu':{key:snapshot[key] for key in
                   ('cache_structure_bytes','parameter_payload_bytes','preparation_bytes','output_parameters')},
            'limits':['CPU owner categories are reported separately and are not summed.',
                      'Checkpoint versions and roles may alias the same allocation.']}


def _gpu_indices(node, count):
    result = set()
    for kind in ('nested', 'shared'):
        start, end = node.get(f'{kind}_gpu_start'), node.get(f'{kind}_gpu_end')
        if start is None and end is None:
            continue
        if (type(start) is not int or type(end) is not int
                or start < 0 or end < start or end > count):
            raise ValueError('invalid node GPU timing index range')
        result.update(range(start, end))
    return result


def _observed_nodes(profile, graph):
    graph_id = graph['id']
    result = {}
    for node in profile['nodes']:
        if node['graph_id'] != graph_id:
            continue
        ordinal = node['ordinal']
        if ordinal in result:
            raise ValueError('duplicate ordinal within graph invocation')
        result[ordinal] = node
    return result


def ancestry(graph, output_ids):
    """Walk graph-local output IDs and return the unique contributing nodes."""
    outputs = {output['id']:output for output in graph['outputs']}
    if len(outputs) != len(graph['outputs']):
        raise ValueError('duplicate graph output id')
    done, visiting, producers = set(), set(), set()
    def visit(output_id):
        if output_id in done:
            return
        if output_id in visiting:
            raise ValueError('cyclic output dependency')
        if output_id not in outputs:
            raise ValueError('missing graph output dependency')
        visiting.add(output_id)
        output = outputs[output_id]
        for dependency in output['dependencies']:
            visit(dependency)
        visiting.remove(output_id)
        done.add(output_id)
        producers.add(output['producer'])
    for output_id in output_ids:
        visit(output_id)
    return {'output_ids':sorted(done), 'node_ordinals':sorted(producers)}


def theoretical_avoidance(profile, graph, output_ids):
    """Describe dependency-union GPU measurements, not an implemented cache policy."""
    path = ancestry(graph, output_ids)
    observed = _observed_nodes(profile, graph)
    indices, missing = set(), []
    for ordinal in path['node_ordinals']:
        node = observed.get(ordinal)
        if node is None or not node['executed']:
            missing.append(ordinal)
        elif node:
            indices.update(_gpu_indices(node, len(profile['gpu'])))
    output_observations = {output['id']:output for node in observed.values() for output in node['outputs']}
    missing_outputs = [output_id for output_id in path['output_ids']
                       if (output_id not in output_observations
                           or not output_observations[output_id].get('requested_for_execution', False)
                           or not output_observations[output_id].get('published', False))]
    elapsed = []
    for index in sorted(indices):
        value = profile['gpu'][index]['milliseconds']
        if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
            raise ValueError('invalid GPU stage timing')
        elapsed.append(value)
    return {**path, 'gpu_timing_indices':sorted(indices),
            'observed_ancestor_gpu_ms':sum(elapsed) if indices else None,
            'unobserved_node_ordinals':missing, 'unobserved_output_ids':missing_outputs,
            'complete_ancestor_observation':not missing and not missing_outputs,
            'interpretation':'Theoretical ancestry only; producer timing includes all sibling outputs. Other roots, cache boundaries and CPU preparation can change actual rebuild work.'}


def analyze_profile(profile, snapshot):
    """Keep each graph's identities and output resources within its profile invocation."""
    inventory = cache_inventory(snapshot)
    aliases = defaultdict(list)
    for item in inventory['allocations']:
        for alias in item['aliases']:
            aliases[(item['allocation_id'], alias['generation'])].append(alias)
    graphs = []
    graph_ids = set()
    for graph in profile['graphs']:
        graph_id = graph['id']
        if graph_id in graph_ids:
            raise ValueError('duplicate graph id in profile')
        graph_ids.add(graph_id)
        nodes = _observed_nodes(profile, graph)
        observations = {}
        for node in nodes.values():
            for output in node['outputs']:
                if output['id'] in observations:
                    raise ValueError('duplicate observed graph output id')
                observations[output['id']] = output
        candidates = []
        for output in graph['outputs']:
            observation = observations.get(output['id'])
            resource = observation.get('resource') if observation else None
            producer = nodes.get(output['producer'])
            candidate = {'output_id':output['id'], 'name':output['name'], 'producer':output['producer'],
                         'dependencies':output['dependencies'], 'data_type':output['data_type'],
                         'session_hint':output['session_hint'], 'sealed_cut':output['sealed_cut'],
                         'resource':resource, 'observation':observation, 'checkpoint_aliases':aliases.get(allocation_key(resource), []) if resource else [],
                         'producer_operation':producer.get('operation') if producer else None,
                         'producer_host_ms':producer.get('host_milliseconds') if producer else None,
                         'ancestor_gpu':theoretical_avoidance(profile, graph, [output['id']])}
            candidates.append(candidate)
        ranked = [candidate for candidate in candidates
                  if candidate['resource'] and candidate['resource']['byte_len'] > 0
                  and candidate['ancestor_gpu']['complete_ancestor_observation']
                  and candidate['ancestor_gpu']['observed_ancestor_gpu_ms'] is not None]
        ranked.sort(key=lambda candidate: candidate['ancestor_gpu']['observed_ancestor_gpu_ms'] / candidate['resource']['byte_len'], reverse=True)
        rank = [{'output_id':candidate['output_id'], 'name':candidate['name'],
                 'allocation_id':candidate['resource']['allocation_id'],
                 'generation':candidate['resource']['generation'],
                 'bytes':candidate['resource']['byte_len'],
                 'ancestor_gpu_ms':candidate['ancestor_gpu']['observed_ancestor_gpu_ms'],
                 'ancestor_gpu_ms_per_mib':candidate['ancestor_gpu']['observed_ancestor_gpu_ms'] * (1024 * 1024) / candidate['resource']['byte_len']}
                for candidate in ranked]
        selected = [candidate for candidate in candidates if any(alias['selected'] for alias in candidate['checkpoint_aliases'])]
        selected_union = {'output_ids':[candidate['output_id'] for candidate in selected],
                          'storage':unique_allocations([candidate['resource'] for candidate in selected]),
                          'ancestor_gpu':theoretical_avoidance(profile, graph, [candidate['output_id'] for candidate in selected]) if selected else None}
        node_boundaries = []
        for node in nodes.values():
            indices = sorted(_gpu_indices(node, len(profile['gpu']))) if node['executed'] else []
            node_boundaries.append({
                'ordinal':node['ordinal'], 'operation':node['operation'], 'executed':node['executed'],
                'started_ms':node.get('started_ms'), 'host_milliseconds':node.get('host_milliseconds'), 'total_ms':node.get('total_ms'),
                'publication_ms':node.get('publication_ms'), 'completion_wait_ms':node.get('completion_wait_ms'),
                'retirement_ms':node.get('retirement_ms'), 'diagnostic_overhead_ms':node.get('diagnostic_overhead_ms'),
                'gpu_timing_indices':indices,
                'gpu_ms':sum(profile['gpu'][i]['milliseconds'] for i in indices) if indices else None,
                'memory_before':node.get('memory_before'), 'memory_after_publication':node.get('memory_after_publication'),
                'memory_after_retirement':node.get('memory_after_retirement')})
        graphs.append({'graph_id':graph_id, 'requested_roots':graph['requested_roots'],
                       'boundary_memory_scope':graph.get('boundary_memory_scope'),
                       'candidates':candidates, 'theoretical_rank':rank, 'selected_union':selected_union,
                       'node_boundaries':node_boundaries})
    return {'graphs':graphs, 'checkpoint_inventory':inventory,
            'limits':['Allocations and timings are scoped to this profile/session snapshot.',
                      'GPU timings are deduplicated by timing index; absent timings are unknown, not zero. Stage sums are not elapsed wall time.',
                      'Rank is a theoretical ancestry GPU-time-per-byte comparison, not a tested retention recommendation. Ranked outputs may share storage.',
                      'Overlapping flag totals are not additive.',
                      'Allocation identity, not write generation, determines physical bytes.',
                      'Boundary memory is not a transient GPU or process-footprint peak.',
                      'Graph ancestry excludes CPU recipe and metadata preparation.']}


def analyze_records(records):
    """Analyze independent trial/control phases from the checkpoint JSONL envelope."""
    import statistics
    sessions, session_modes, renders, verifies = {}, {}, {}, {}
    phases = ('base', 'unchanged', 'edit-a', 'edit-b', 'return-a')
    for record in records:
        if not isinstance(record, dict) or record.get('foveonic_profile') != 1:
            raise ValueError('invalid checkpoint profiling envelope')
        event, mode, data = record.get('event'), record.get('mode'), record.get('data')
        if not isinstance(data, dict) or mode not in ('off', 'wall', 'gpu'):
            raise ValueError('invalid checkpoint profiling event')
        if event == 'checkpoint_session':
            case = data['case']
            if case in sessions:
                raise ValueError('duplicate checkpoint session case')
            repeats, verify, controls = data.get('repeats'), data.get('verify'), data.get('controls')
            if (type(repeats) is not int or repeats < 1 or type(verify) is not bool
                    or not isinstance(controls, list) or not controls
                    or any(not isinstance(control, str) or not control for control in controls)
                    or len(set(controls)) != len(controls)
                    or type(data.get('timing_usable')) is not bool
                    or data['timing_usable'] != (not verify and mode == 'off')):
                raise ValueError('checkpoint session needs declared controls, positive repeats and verification/timing flags')
            sessions[case] = data
            session_modes[case] = mode
        elif event in ('checkpoint_render', 'checkpoint_verify'):
            key = (data['case'], data['control'], data['trial'], data['phase'])
            if data['phase'] not in phases:
                raise ValueError('unknown checkpoint render phase')
            target = renders if event == 'checkpoint_render' else verifies
            if key in target:
                raise ValueError('duplicate checkpoint trial/phase event')
            target[key] = (mode, data)
        else:
            raise ValueError(f'unknown checkpoint event: {event}')
    if not sessions:
        raise ValueError('checkpoint report needs a session event')
    if any(key[0] not in sessions for key in renders) or any(key not in renders for key in verifies):
        raise ValueError('checkpoint event lacks its session or render')
    groups = defaultdict(list)
    for case, session in sessions.items():
        for control in session['controls']:
            groups[(case, control, session_modes[case])] = []
    for (case, control, trial, phase), (mode, row) in renders.items():
        if control not in sessions[case]['controls'] or mode != session_modes[case]:
            raise ValueError('checkpoint render does not match its declared control or mode')
        for name in ('completed_ms','render_call_ms'):
            value = row[name]
            if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
                raise ValueError(f'invalid {name}')
        profile = dict(row['profile'])
        profile.setdefault('graphs', [])
        profile.setdefault('nodes', [])
        profile.setdefault('gpu', [])
        analysis = analyze_profile(profile, row['checkpoints'])
        verify = verifies.get((case, control, trial, phase))
        if verify is not None and verify[0] != mode:
            raise ValueError('checkpoint verification mode does not match its render')
        groups[(case, control, mode)].append({
            'trial':trial, 'phase':phase, 'settings':row['settings'],
            'completed_ms':row['completed_ms'], 'render_call_ms':row['render_call_ms'],
            'activity':row['activity'], 'resources':row['resources'],
            'engine_preparation_bytes':row['engine_preparation_bytes'],
            'rendered_image':row['rendered_image'],
            'verification':verify[1] if verify else None, 'analysis':analysis,
            'diagnostic_evidence_complete':mode == 'off' or bool(profile['graphs'] and profile['nodes'])})
    cases = []
    for (case, control, mode), rows in sorted(groups.items()):
        by_trial = defaultdict(dict)
        expected_repeats = sessions[case]['repeats']
        for row in rows:
            if type(row['trial']) is not int or not 0 <= row['trial'] < expected_repeats:
                raise ValueError('checkpoint trial is outside the declared repeats')
            by_trial[row['trial']][row['phase']] = row
        incomplete = [{'trial':trial, 'missing':[phase for phase in phases if phase not in by_trial[trial]]}
                      for trial in range(expected_repeats)
                      if any(phase not in by_trial[trial] for phase in phases)]
        required = sessions[case]['verify']
        failed_verification = []
        missing_verification = []
        for trial in range(expected_repeats):
            for phase in phases:
                verification = by_trial[trial].get(phase, {}).get('verification')
                if verification is not None and verification.get('equal') is False:
                    failed_verification.append({'trial':trial, 'phase':phase})
                if (required or verification is not None) and (not isinstance(verification, dict)
                                 or type(verification.get('equal')) is not bool
                                 or verification.get('outside_timing') is not True):
                    missing_verification.append({'trial':trial, 'phase':phase})
        verification_status = ('failed' if failed_verification else
                               'incomplete' if missing_verification else
                               'passed' if required else 'not_requested')
        phase_timings = []
        for phase in phases:
            matched = [row for row in rows if row['phase'] == phase]
            if matched:
                phase_timings.append({'phase':phase, 'trials':len(matched),
                    'completed_median_ms':statistics.median(row['completed_ms'] for row in matched),
                    'render_call_median_ms':statistics.median(row['render_call_ms'] for row in matched)})
        missing_diagnostics = [{'trial':row['trial'], 'phase':row['phase']} for row in rows
                               if not row['diagnostic_evidence_complete']]
        cases.append({'case':case, 'control':control, 'mode':mode, 'session':sessions[case],
                      'phases':sorted(rows, key=lambda row:(row['trial'], phases.index(row['phase']))),
                      'phase_timings':phase_timings, 'incomplete_trials':incomplete,
                      'performance_evidence_complete':not incomplete,
                      'diagnostic_evidence_complete':not incomplete and not missing_diagnostics,
                      'missing_diagnostics':missing_diagnostics,
                      'session_timing_usable':sessions[case]['timing_usable'],
                      'performance_usable':not incomplete and sessions[case]['timing_usable'] and mode == 'off',
                      'verification':{'status':verification_status, 'required':required,
                                      'missing_or_invalid':missing_verification, 'failed':failed_verification}})
    return {'schema_version':1, 'cases':cases,
            'capture_complete':bool(cases) and all(case['performance_evidence_complete']
                                                   and case['diagnostic_evidence_complete']
                                                   and not case['verification']['missing_or_invalid'] for case in cases),
            'verification_failed':any(case['verification']['failed'] for case in cases),
            'limits':['Each trial starts with a fresh session and executor; compiled shaders are warmed separately.',
                      'Base and edited graphs are separate observations. Phase-level rebuild times do not establish a checkpoint-specific saving.',
                      'Synchronized GPU diagnostic timings are attribution, not unsynchronized latency.',
                      'Checkpoint snapshots and verification are outside completed render timing.',
                      'Unobserved or absent verification is not evidence of numerical equality.']}


def main():
    import argparse
    import json
    from pathlib import Path
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('input', type=Path, help='profile_checkpoints JSONL capture')
    parser.add_argument('--output', type=Path, help='new report path; defaults to stdout')
    args = parser.parse_args()
    try:
        with args.input.open() as stream:
            records = [json.loads(line) for line in stream if line.strip()]
        result = analyze_records(records)
        text = json.dumps(result, indent=2, allow_nan=False) + '\n'
        if args.output:
            with args.output.open('x') as output:
                output.write(text)
        else:
            print(text, end='')
        if result['verification_failed']:
            parser.exit(1, 'checkpoint verification reported unequal pixels\n')
        if not result['capture_complete']:
            parser.exit(1, 'checkpoint capture is incomplete\n')
    except (ValueError, KeyError, OSError, json.JSONDecodeError) as error:
        parser.exit(1, str(error) + '\n')


if __name__ == '__main__':
    main()
