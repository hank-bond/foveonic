"""Explain observed graph rebuilds using task timing and scalar checkpoint snapshots."""

from bisect import bisect_right
from collections import defaultdict
import json

from checkpoints import unique_allocations


IDENTITY = ('task', 'revision', 'photo', 'purpose')


def _identity(event):
    return tuple(event['args'].get(key) for key in IDENTITY)


def _array(value):
    value = json.loads(value) if isinstance(value, str) else value
    if not isinstance(value, list):
        raise ValueError('checkpoint trace array is malformed')
    return value


def explain(events, work, memory, recorded_history_valid):
    """Describe observed availability and work, without simulating another policy."""
    headers, outputs, snapshots, nodes = {}, defaultdict(list), {}, defaultdict(list)
    complete, completed_at, history = set(), {}, defaultdict(list)
    tasks = {}
    for event in events:
        fields = event['args']
        graph = fields.get('graph_id')
        if event['name'] in ('graph.execute', 'graph.prepare_execution') and graph is not None:
            if graph in headers:
                raise ValueError('duplicate graph identity')
            headers[graph] = event
        elif event['name'] == 'graph.output' and graph is not None:
            outputs[graph].append(event)
        elif event['name'] == 'cache.checkpoint.snapshot' and graph is not None:
            if graph in snapshots:
                raise ValueError('duplicate graph checkpoint snapshot')
            snapshots[graph] = event
        elif event['name'] == 'graph.complete' and graph is not None:
            complete.add(graph)
            completed_at[graph] = event['ts']
        if event['name'] in ('cache.checkpoint.admitted', 'cache.checkpoint.evicted'):
            if fields.get('cache_id') is not None and fields.get('generation') is not None:
                history[(fields['cache_id'], fields['generation'])].append(event)
        key = _identity(event)
        if fields.get('task'):
            task = tasks.setdefault(key, {**dict(zip(IDENTITY, key)), 'graphs':[],
                                         'observations':[], 'processing':[], 'memory_boundaries':[]})
            if event['name'] == 'image_frame_use':
                task.setdefault('first_native_encoding_ns', event['ts'])
                task['native_encoding_count'] = task.get('native_encoding_count', 0) + 1
                continue
            if event['name'] in ('render_task', 'render_queue', 'request_to_ui',
                                  'image_accepted', 'completed',
                                  'completion_superseded', 'completion_removed'):
                task['observations'].append({'event':event['name'], 'ts_ns':event['ts'],
                    'elapsed_ms':(fields['duration_ns'] / 1e6 if fields.get('interval_kind') == 'elapsed_cpu_clock'
                                  else event['dur'] / 1e6 if event['dur'] >= 0 else None)})
            elif event['name'] == 'desired':
                task['request'] = {name:fields[name] for name in
                                   ('preview_scale', 'processing_quality', 'nominal_scale', 'output') if name in fields}
            elif event['name'] == 'processing':
                facts = {name:fields[name] for name in
                    ('source', 'execution', 'status', 'scale', 'output', 'cache_before_bytes', 'cache_after_bytes')
                    if name in fields}
                if facts:
                    task['processing'].append({'ts_ns':event['ts'], **facts})
    for rows in history.values():
        rows.sort(key=lambda event:event['ts'])
    for node in work['nodes']:
        if node.get('graph_id') is not None:
            nodes[node['graph_id']].append(node)
    for event in events:
        if event['name'] == 'memory.snapshot' and _identity(event) in tasks:
            tasks[_identity(event)]['memory_boundaries'].append(
                {'ts_ns':event['ts'], **event['args']})

    graphs = []
    for graph_id, header in sorted(headers.items(), key=lambda item:item[1]['ts']):
        fields, issues = header['args'], []
        cache_id = fields.get('cache_id')
        inventory = None
        snapshot = snapshots.get(graph_id)
        if cache_id is not None:
            if snapshot is None or snapshot['args'].get('cache_id') != cache_id:
                issues.append('missing matching entry checkpoint snapshot')
            else:
                entries = _array(snapshot['args']['entries'])
                inventory = {**unique_allocations(entries), 'entries':entries,
                             'ts_ns':snapshot['ts'], 'cache_frame':snapshot['args'].get('cache_frame')}
        seen, by_producer = set(), defaultdict(list)
        for event in outputs[graph_id]:
            output = event['args']
            output_id = output.get('output_id')
            if output_id in seen:
                issues.append('duplicate output observation')
            seen.add(output_id)
            if (type(output_id) is not int or type(output.get('producer')) is not int
                    or any(type(output.get(key)) is not bool for key in
                           ('required', 'requested_for_execution', 'published', 'seeded', 'present'))):
                issues.append('missing output demand metadata')
                continue
            dependencies = _array(output['dependencies'])
            if any(type(value) is not int for value in dependencies):
                raise ValueError('invalid graph output dependency')
            if cache_id is not None and output.get('cache_id') != cache_id:
                issues.append('output belongs to a different cache')
            if cache_id is not None and type(output.get('cache_version')) is not int:
                issues.append('missing output cache version')
            row = {key:output.get(key) for key in
                   ('output_id', 'name', 'cache_version', 'required', 'requested_for_execution',
                    'published', 'seeded', 'present', 'allocation_id', 'allocation_generation',
                    'byte_len', 'working_scale')}
            row['producer'] = output['producer']
            row['dependencies'] = dependencies
            if output['requested_for_execution']:
                previous = history.get((cache_id, output.get('cache_version')), [])
                index = bisect_right([event['ts'] for event in previous], header['ts']) - 1
                prior = previous[index] if index >= 0 else None
                row['prior_eviction'] = (
                    {'ts_ns':prior['ts'], 'reason':prior['args'].get('reason'),
                     'buffer_reference_bytes':prior['args'].get('buffer_reference_bytes')}
                    if prior is not None and prior['name'] == 'cache.checkpoint.evicted' else None)
                row['recorded_history_valid'] = recorded_history_valid
            by_producer[output['producer']].append(row)
        if type(fields.get('outputs')) is not int or seen != set(range(fields['outputs'])):
            issues.append('incomplete graph output observations')
        if any(dependency not in seen for rows in by_producer.values()
               for output in rows for dependency in output['dependencies']):
            issues.append('missing graph dependency observations')
        if (type(fields.get('nodes')) is not int
                or {node['node'] for node in nodes[graph_id]} != set(range(fields['nodes']))):
            issues.append('incomplete graph node observations')
        if graph_id not in complete or header['dur'] < 0:
            issues.append('graph completion not observed')
        roots = _array(fields['requested_roots']) if 'requested_roots' in fields else []
        if 'requested_roots' not in fields:
            issues.append('missing requested graph roots')
        elif any(root not in seen for root in roots):
            issues.append('requested root output not observed')
        executed, reentries = [], []
        for node in nodes[graph_id]:
            if not node['executed']:
                continue
            observed = by_producer[node['node']]
            if len(observed) != node.get('output_count'):
                issues.append('incomplete producer output observations')
            row = {key:node.get(key) for key in
                   ('slice', 'node', 'operation', 'inclusive_host_ms', 'dispatches',
                    'submissions', 'gpu_pass_ms', 'gpu_passes_observed')}
            row['missing_required_outputs'] = [output for output in observed if output['requested_for_execution']]
            row['available_sibling_outputs'] = [output for output in observed if output['seeded']]
            executed.append(row)
            if row['missing_required_outputs'] and row['available_sibling_outputs']:
                reentries.append(row['node'])
        result = {'graph_id':graph_id, 'cache_id':cache_id,
                  **dict(zip(IDENTITY, _identity(header))),
                  'ts_ns':header['ts'], 'scope_event':header['name'],
                  'inclusive_host_ms':header['dur'] / 1e6 if header['name']=='graph.execute' and header['dur'] >= 0 else None,
                  'preparation_host_ms':header['dur'] / 1e6 if header['name']=='graph.prepare_execution' and header['dur'] >= 0 else None,
                  'execution_elapsed_ms':(completed_at[graph_id]-header['ts']) / 1e6 if graph_id in completed_at else None,
                  'local_evidence_complete':not issues, 'issues':sorted(set(issues)),
                  'entry_checkpoints':inventory, 'executed_nodes':executed,
                  'requested_roots':roots,
                  'outputs':sorted((output for rows in by_producer.values() for output in rows),
                                   key=lambda output:output['output_id']),
                  'producers_with_cached_siblings':reentries}
        graphs.append(result)
        if _identity(header) in tasks:
            tasks[_identity(header)]['graphs'].append(graph_id)
    orphans = (set(outputs) | set(snapshots) | set(nodes) | complete) - headers.keys()
    return {'graphs':graphs, 'tasks':[task for task in tasks.values()
                                   if task['graphs'] or task['observations'] or task['processing']
                                   or task['memory_boundaries'] or task.get('request')
                                   or task.get('native_encoding_count')],
            'unidentified_graph_records':sum(
                event['name'] in ('graph.execute', 'graph.prepare_execution', 'graph.output', 'graph.complete', 'cache.checkpoint.snapshot')
                and event['args'].get('graph_id') is None for event in events)
                + sum(node.get('graph_id') is None for node in work['nodes']),
            'orphan_graph_ids':sorted(orphans), 'recorded_history_valid':recorded_history_valid,
            'history_before_capture':'unknown',
            'memory_boundaries':memory,
            'limits':['This report explains observed work, not savings under another cache policy.',
                      'A resumable graph preparation slice measures initialization only. Execution elapsed time includes interleaving, waits and service of other jobs, not CPU-active time.',
                      'Available outputs are not necessarily required. Cached siblings do not prevent work for missing outputs.',
                      'History before capture is unknown even in a loss-free recording. No prior eviction observation is not proof of eviction or invalidation.',
                      'Snapshot and eviction bytes reference allocations. Aliases count once and eviction does not imply freed memory.',
                      'Worker memory boundaries are budget observations, not process footprint or driver residency.',
                      'Host node durations are inclusive. GPU timings retain their own clock and are not elapsed interaction savings.',
                      'Incomplete graphs and missing history cannot establish a complete causal explanation.']}
