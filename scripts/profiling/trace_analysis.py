"""Query real Perfetto captures for loss, scenario correctness and shared-clock latency."""

import argparse
from collections import Counter, defaultdict
import csv
import io
import json
from pathlib import Path
import statistics
import subprocess
from file_hash import digest


# Hex transports text through the CLI CSV writer, including JSON quotes/newlines.
# Numeric fields remain native SQL values. Nullable text preserves SQL NULL.
QUERIES = {
    'slices': 'SELECT id,ts,dur,track_id,parent_id,depth,hex(name) AS name_hex,hex(category) AS category_hex,arg_set_id FROM slice ORDER BY ts,id',
    'args': "SELECT arg_set_id,hex(key) AS key_hex,hex(value_type) AS value_type_hex,int_value,real_value,CASE WHEN string_value IS NULL THEN NULL ELSE hex(string_value) END AS string_value_hex FROM args WHERE key GLOB 'debug.*' ORDER BY arg_set_id,id",
    'stats': 'SELECT hex(name) AS name_hex,idx,hex(severity) AS severity_hex,hex(source) AS source_hex,value FROM stats ORDER BY name,idx',
    'counters': 'SELECT c.ts,c.value,c.track_id,hex(t.name) AS name_hex FROM counter c JOIN counter_track t ON t.id=c.track_id ORDER BY c.ts',
    'flows': 'SELECT id,slice_out,slice_in,trace_id FROM flow ORDER BY id',
    'tracks': 'SELECT t.id,hex(t.name) AS name_hex,th.utid,th.tid,CASE WHEN th.name IS NULL THEN NULL ELSE hex(th.name) END AS thread_name_hex,p.upid,p.pid,CASE WHEN p.name IS NULL THEN NULL ELSE hex(p.name) END AS process_name_hex FROM track t LEFT JOIN thread_track tt ON tt.id=t.id LEFT JOIN thread th ON th.utid=tt.utid LEFT JOIN process_track pt ON pt.id=t.id LEFT JOIN process p ON p.upid=coalesce(th.upid,pt.upid) ORDER BY t.id',
}
NULL = {'[NULL]', 'NULL'}


def csv_tables(text):
    """A CSV reader preserves quoted multiline values while separating result sets."""
    tables, current, header = [], [], None
    for row in csv.reader(io.StringIO(text)):
        if not row:
            if header is not None:
                tables.append(current)
            current, header = [], None
        elif header is None:
            header = row
        else:
            if len(row) != len(header):
                raise ValueError('Trace Processor returned a truncated CSV row')
            current.append(dict(zip(header, row)))
    if header is not None:
        tables.append(current)
    return tables


def decode_transport(tables):
    """Decode only named text columns after the CSV structure has been parsed."""
    return [[{key.removesuffix('_hex') if key.endswith('_hex') else key:
              (None if value in NULL else bytes.fromhex(value).decode('utf-8'))
              if key.endswith('_hex') else value
              for key, value in row.items()} for row in rows] for rows in tables]


def query_trace(tool, trace, evidence=None):
    tool, trace = Path(tool).resolve(strict=True), Path(trace).resolve(strict=True)
    query = ';\n'.join(QUERIES.values()) + ';'
    result = subprocess.run([str(tool), 'query', '--format', 'csv', str(trace), query],
                            text=True, capture_output=True, check=True)
    tables = decode_transport(csv_tables(result.stdout))
    if len(tables) != len(QUERIES):
        raise ValueError('Trace Processor did not return every requested table')
    if evidence is not None:
        evidence = Path(evidence)
        evidence.mkdir(parents=True, exist_ok=True)
        (evidence / 'queries.sql').write_text(query + '\n')
        (evidence / 'queries.csv').write_text(result.stdout)
        (evidence / 'trace-processor.stderr').write_text(result.stderr)
    return dict(zip(QUERIES, tables))


def typed_arg(row):
    kind = row['value_type']
    if kind in ('int', 'uint', 'bool'):
        value = int(row['int_value'])
        if kind == 'uint' and value < 0:
            value += 1 << 64
        return bool(value) if kind == 'bool' else value
    if kind in ('real', 'double'):
        return float(row['real_value'])
    if kind == 'string':
        return row['string_value']
    return None


def events_from_tables(tables):
    args = defaultdict(dict)
    for row in tables['args']:
        key = row['key'].removeprefix('debug.')
        if key in args[row['arg_set_id']]:
            raise ValueError(f'duplicate diagnostic field: {key}')
        args[row['arg_set_id']][key] = typed_arg(row)
    return [{**row, 'ts': int(row['ts']), 'dur': int(row['dur']),
             'args': args.get(row['arg_set_id'], {})} for row in tables['slices']]


def distribution(values):
    values = sorted(values)
    return {'count': len(values), 'median': statistics.median(values) if values else None,
            'p95': values[max(0, (len(values) * 95 + 99) // 100 - 1)] if values else None,
            'maximum': max(values, default=None)}


def loss_status(stats, complete):
    problems = [row for row in stats if int(row['value']) != 0 and
                (row['severity'] in ('data_loss', 'error') or
                 (row['name'].startswith('traced_buf_') and
                  any(word in row['name'] for word in ('overwritten', 'overrun', 'discarded'))))]
    checked = any(row['name'].startswith('traced_buf_') for row in stats)
    return {'complete': complete is True, 'loss_checked': checked,
            'loss_free': checked and not problems, 'problems': problems,
            'service_warnings': [row for row in stats if int(row['value']) != 0 and row['name'] in
                                 ('traced_chunks_discarded','traced_patches_discarded','traced_invalid_packets')],
            'valid': complete is True and checked and not problems}


def cancelled_prefetch_reads(events):
    """Require the read's own prefetch scope to report an explicit interruption."""
    indexed = {str(event['id']): event for event in events if 'id' in event}
    outcomes = {str(event.get('parent_id')): event for event in events
                if event['name'] == 'cache.processing.prefetch_result'}
    misses = {str(event.get('parent_id')): event for event in events
              if event['name'] == 'cache.decoded.miss'}
    cancelled = set()
    for event in events:
        fields = event['args']
        if (event['name'] != 'source.read.complete' or fields.get('task') != 0
                or fields.get('purpose') != 'thumb_prefetch'
                or fields.get('storage') != 'decoded-cache'
                or fields.get('complete') is not False or fields.get('read_errors') != 0):
            continue
        parent = str(event.get('parent_id'))
        miss = None
        while parent in indexed:
            scope = indexed[parent]
            if scope['name'] == 'cache.decoded.lookup':
                miss = misses.get(parent)
            if scope['name'] == 'cache.processing.prefetch':
                outcome = outcomes.get(parent)
                if (outcome and outcome['args'].get('outcome') == 'interrupted'
                        and miss and miss['args'].get('reason') == 'background prefetch interrupted'
                        and fields.get('source')
                        and outcome['args'].get('source') == fields['source']
                        and miss['args'].get('source') == fields['source']):
                    cancelled.add(str(event['id']))
                break
            parent = str(scope.get('parent_id'))
    return cancelled


def expected_work(window, expectations, photos, sources=None):
    """Check actual work independently of accepted result identity or latency."""
    payloads = [event for event in window if event['name'] == 'source.read.complete' and
                event['args'].get('storage') in ('original', 'decoded-cache')]
    # Task zero denotes work outside the foreground coordinator. Keep reads
    # of other photos visible without assigning them to the selected targets.
    reads, background, not_started = [], [], []
    for event in payloads:
        fields = event['args']
        # A reader can be cancelled before its first I/O call. Its lifetime is
        # still useful evidence, but it is not a performed payload read.
        if (fields.get('read_operations') == 0 and not fields.get('bytes', 0)
                and not fields.get('read_errors', 0)):
            not_started.append(event)
            continue
        unrelated = (fields.get('task') == 0 and fields.get('photo') is not None
                     and fields['photo'] not in photos)
        (background if unrelated else reads).append(event)
    counts = Counter(event['args']['storage'] for event in reads)
    issues = []
    for kind, field in [('original', 'expect_original_reads'), ('decoded-cache', 'expect_local_reads')]:
        expected = expectations.get(field)
        if expected is not None and counts[kind] != expected:
            issues.append(f'{kind} payload reads: expected {expected}, observed {counts[kind]}')
    cancelled = cancelled_prefetch_reads(window)
    cancelled_reads = [event for event in reads if str(event.get('id')) in cancelled]
    if any(event['args'].get('read_errors', 0) or
           (event['args'].get('complete') is False and str(event.get('id')) not in cancelled)
           for event in reads):
        issues.append('a source payload read failed or was incomplete')
    dispatches = [event for event in window if event['name'] == 'gpu.encode.dispatch' and
                  event['args'].get('purpose') in ('current', 'camera', 'defaults', 'detail', 'inspection')]
    expected = expectations.get('expect_photographic_dispatches')
    if expected is not None and len(dispatches) != expected:
        issues.append(f'photographic dispatches: expected {expected}, observed {len(dispatches)}')
    # Explicit behavioral expectations are target-scoped. Keep unknown identities
    # in the target set so missing provenance cannot hide work.
    target_reads = [event for event in reads if event['args'].get('photo') is None
                    or event['args']['photo'] in photos]
    target_dispatches = [event for event in dispatches if event['args'].get('photo') is None
                         or event['args']['photo'] in photos]
    declared = expectations.get('expected_work')
    if declared is not None and declared not in ('reuse', 'rebuild'):
        issues.append(f'unknown expected_work: {declared}')
    if declared == 'reuse':
        if target_reads:
            issues.append(f'target payload reads during reuse: {len(target_reads)}')
        if target_dispatches:
            issues.append(f'target photographic dispatches during reuse: {len(target_dispatches)}')
    proof = None
    if declared == 'rebuild':
        source = expectations.get('evicted_source')
        def snapshot(name):
            value = expectations.get(name)
            try:
                value = json.loads(value) if isinstance(value, str) else value
            except (ValueError, TypeError):
                return None
            return value if isinstance(value, list) and all(isinstance(path, str) for path in value) else None
        before, after = snapshot('retained_before'), snapshot('retained_after')
        proof = {'source': source, 'retained_before': before, 'retained_after': after}
        target_photo = (sources or {}).get(source)
        if (not isinstance(source, str) or before is None or after is None or
                source not in before or source in after or target_photo is None):
            issues.append('rebuild requires runtime snapshots proving eviction of a current source')
        if target_photo is None or not any(event['args'].get('photo') == target_photo for event in dispatches):
            issues.append('rebuild had no photographic dispatch for the evicted target')
    def work_identity(event):
        return {key: event['args'].get(key) for key in
                ('photo', 'task', 'revision', 'purpose', 'storage') if key in event['args']}
    return {'expected_work': declared,
            'eviction_evidence': proof,
            'target_payload_read_counts': dict(Counter(event['args']['storage'] for event in target_reads)),
            'target_payload_reads': [work_identity(event) for event in target_reads],
            'target_photographic_dispatch_count': len(target_dispatches),
            'target_photographic_dispatches': [work_identity(event) for event in target_dispatches],
            'payload_read_counts': dict(counts),
            'payload_attempts_without_io': len(not_started),
            'cancelled_payload_read_count': len(cancelled_reads),
            'cancelled_payload_read_bytes': sum(event['args'].get('bytes', 0) for event in cancelled_reads),
            'payload_read_bytes': sum(event['args'].get('bytes', 0) for event in reads),
            'background_payload_read_counts': dict(Counter(event['args']['storage'] for event in background)),
            'background_payload_read_bytes': sum(event['args'].get('bytes', 0) for event in background),
            'photographic_dispatch_count': len(dispatches)}, issues


def analyze_smoke(events):
    """Validate existing assertion checkpoints, without imposing render arrival order."""
    markers = [event for event in events if event['name'] == 'smoke.step']
    terminals = [event for event in events if event['name'] == 'smoke.completed']
    fatal = [event for event in events if event['name'] == 'smoke.failed']
    if not markers and not terminals and not fatal:
        return None
    failures = [f"smoke failed: {event['args'].get('reason', 'unspecified fatal assertion')}" for event in fatal]
    steps = []
    suites = {event['args'].get('suite') for event in markers + terminals}
    for suite in sorted(suites, key=str):
        done = [event for event in terminals if event['args'].get('suite') == suite]
        rows = [event for event in markers if event['args'].get('suite') == suite]
        counts = {event['args'].get('step_count') for event in rows + done}
        if suite is None or len(done) != 1 or len(counts) != 1:
            failures.append(f'smoke {suite}: missing/duplicate completion or inconsistent step count')
            continue
        count = counts.pop()
        if not isinstance(count, int) or count < 1 or done[0]['args'].get('checked_steps') != count:
            failures.append(f'smoke {suite}: incomplete assertion checkpoints')
            continue
        if {event['args'].get('step') for event in rows} != set(range(1, count + 1)):
            failures.append(f'smoke {suite}: missing assertion steps')
        for step in range(1, count + 1):
            current = [event for event in rows if event['args'].get('step') == step]
            begin = [event for event in current if event['args'].get('phase') == 'begin']
            checked = [event for event in current if event['args'].get('phase') == 'checked']
            if len(begin) != 1 or len(checked) != 1:
                failures.append(f'smoke {suite} step {step}: missing/duplicate begin or checked marker')
                continue
            start, end = begin[0], checked[0]
            if (start['args'].get('target') != end['args'].get('target') or
                    end['ts'] < start['ts'] or end['ts'] > done[0]['ts']):
                failures.append(f'smoke {suite} step {step}: mismatched target or checkpoint bounds')
            work, issues = expected_work([event for event in events if start['ts'] <= event['ts'] <= end['ts']],
                                        start['args'], {end['args'].get('photo')})
            failures.extend(f'smoke {suite} step {step}: {issue}' for issue in issues)
            steps.append({**work,'suite':suite,'step':step,'target':start['args'].get('target'),
                          'coverage':start['args'].get('coverage'),'mode':start['args'].get('mode'),
                          'elapsed_including_harness_ms':(end['ts']-start['ts'])/1e6,
                          'worker_busy_at_check':end['args'].get('worker_busy'),
                          'displayed':{key:end['args'][key] for key in
                                       ('task','revision','photo','presentation_revision','artifact','scale_divisor')
                                       if key in end['args']}})
    return {'valid':not failures,'failures':failures,'steps':steps,
            'limits':['Existing smoke assertions define readiness for each target, including intentional pending/preview checks.',
                      'Step durations include screenshot readback and artifact encoding and are not latency baselines.']}


IDENTITY_FIELDS = ('task', 'revision', 'photo', 'purpose')


def optional_int(value):
    return None if value is None or value in NULL else int(value)


def identity(event):
    return tuple(event['args'].get(key) for key in ('task', 'revision', 'photo'))


def scope_attribution(events, tables):
    """Keep recorded parent/thread structure and inclusive elapsed attribution."""
    by_id = {optional_int(event.get('id')):event for event in events}
    tracks = {optional_int(row['id']):row for row in tables.get('tracks', [])}
    hierarchy, stages = [], defaultdict(lambda: defaultdict(list))
    for event in events:
        if event['dur'] <= 0:
            continue
        fields = event['args']
        parent_id = optional_int(event.get('parent_id'))
        ancestors, seen, parent = [], set(), parent_id
        while parent is not None and parent in by_id and parent not in seen:
            seen.add(parent)
            ancestor = by_id[parent]
            ancestors.append({'slice':parent,'name':ancestor['name']})
            parent = optional_int(ancestor.get('parent_id'))
        ancestors.reverse()
        track = tracks.get(optional_int(event['track_id']), {})
        context = {key:fields[key] for key in (*IDENTITY_FIELDS,'node') if key in fields}
        attributes = {key:value for key,value in fields.items() if key not in context}
        hierarchy.append({'slice':optional_int(event.get('id')),'parent_id':parent_id,
                          'depth':optional_int(event.get('depth')),'track':optional_int(event['track_id']),
                          'thread':{key:(optional_int(track[key]) if key in ('utid','tid') else track[key]) for key in ('utid','tid','thread_name') if track.get(key) not in (None,'[NULL]')},
                          'process':{key:(optional_int(track[key]) if key in ('upid','pid') else track[key]) for key in ('upid','pid','process_name') if track.get(key) not in (None,'[NULL]')},
                          'category':event['category'],'name':event['name'],
                          'start_ns':event['ts'],'duration_ms':event['dur']/1e6,
                          'ancestors':ancestors,'context':context,'attributes':attributes})
        if fields.get('task') is not None:
            key=tuple(fields.get(name) for name in IDENTITY_FIELDS)
            stages[key][(event['category'],event['name'])].append(event['dur']/1e6)
    return {'scopes':hierarchy,'task_stage_elapsed_ms':[
                {**dict(zip(IDENTITY_FIELDS,key)),
                 'stages':[{'category':category,'name':name,'inclusive_total':sum(values),**distribution(values)}
                           for (category,name),values in sorted(stage.items())]}
                for key,stage in sorted(stages.items(),key=lambda pair:str(pair[0]))],
            'measurement':'Instrumented elapsed scope hierarchy. Inclusive and nested stage totals are not additive and are not CPU-active samples.'}


def gpu_attribution(events):
    grouped = defaultdict(list)
    keys=(*IDENTITY_FIELDS,'node','submission','clock_id')
    for event in events:
        fields=event['args']
        if all(key in fields for key in ('clock_id','start_tick','end_tick','duration_ns')):
            grouped[tuple(fields.get(key) for key in keys)].append(fields['duration_ns']/1e6)
    return [{**dict(zip(keys,key)),'pass_duration_ms':distribution(values),'pass_duration_total_ms':sum(values)}
            for key,values in sorted(grouped.items(),key=lambda pair:str(pair[0]))]


def node_work_census(events):
    """Attribute actual graph work by recorded ownership, including repeated node indices."""
    by_id = {optional_int(event.get('id')): event for event in events}
    nodes = {key: event for key, event in by_id.items()
             if 'executed' in event['args'] and 'operation' in event['args']}
    rows = {}
    for key, event in nodes.items():
        fields = event['args']
        rows[key] = {'slice': key, **{name: fields.get(name) for name in (*IDENTITY_FIELDS, 'node')},
                     'operation': fields['operation'], 'executed': fields['executed'],
                     'seed_hits': fields.get('seed_hits', 0),
                     'inclusive_host_ms': None if event['dur'] < 0 else event['dur'] / 1e6,
                     'geometry': None, 'dispatches': 0, 'dispatch_grids': [], 'submissions': 0,
                     'submit_host_ms': 0., 'gpu_passes_observed': 0, 'gpu_pass_ms': None}

    def owner(event):
        parent = optional_int(event.get('parent_id'))
        while parent is not None and parent in by_id:
            if parent in nodes:
                return parent
            parent = optional_int(by_id[parent].get('parent_id'))
        return None

    submissions = {}
    for event in events:
        key = owner(event)
        if key is None:
            continue
        row = rows[key]
        fields = event['args']
        if event['name'] == 'graph.geometry':
            row['geometry'] = {name: fields.get(name) for name in
                               ('scale', 'width', 'height', 'input_scale', 'input_width', 'input_height')}
        elif event['name'] == 'gpu.encode.dispatch':
            row['dispatches'] += 1
            row['dispatch_grids'].append({name: fields.get(name) for name in ('kernel', 'groups_x', 'groups_y')})
        elif event['name'] == 'gpu.submit':
            submissions[fields['submission']] = key
            row['submissions'] += 1
            row['submit_host_ms'] += max(0, event['dur']) / 1e6
    unattributed_gpu = 0
    for event in events:
        fields = event['args']
        if not all(name in fields for name in ('clock_id', 'start_tick', 'end_tick', 'duration_ns')):
            continue
        key = submissions.get(fields.get('submission'))
        if key is None:
            unattributed_gpu += 1
            continue
        row = rows[key]
        row['gpu_passes_observed'] += 1
        row['gpu_pass_ms'] = (row['gpu_pass_ms'] or 0.) + fields['duration_ns'] / 1e6
    totals = Counter()
    for row in rows.values():
        if not row['executed']:
            totals['reused_nodes'] += 1
        else:
            scale = (row['geometry'] or {}).get('scale')
            totals['native_nodes' if scale == 1 else 'reduced_nodes' if scale else 'unknown_grid_nodes'] += 1
        totals['dispatches'] += row['dispatches']
        totals['submissions'] += row['submissions']
    return {'nodes': list(rows.values()), 'totals': dict(totals),
            'unattributed_gpu_passes': unattributed_gpu,
            'limits': ['Grid classification uses the executed output geometry, not the requested preview name.',
                       'Reused nodes have no execution geometry. Native nodes are not automatically unnecessary.',
                       'Host intervals are inclusive and GPU observations are separate. Missing GPU measurements are not zero cost.',
                       'Submission ownership follows the recorded scope tree. Node indices may repeat across graphs.']}


def interaction_events(window, begin_ns):
    names = {'scenario.pointer_click', 'input.pointer_button', 'viewport.tile.release', 'viewport.selection.applied'}
    return [{'event': event['name'], 'elapsed_ms': (event['ts'] - begin_ns) / 1e6,
             **event['args']} for event in window if event['name'] in names]


def photo_lifecycle(window, current, begin_ns):
    """Use exact delivered identity, preserving absent receipt for cached reuse."""
    result=[]
    for source, marker in current.items():
        rows=[event for event in window if identity(event)==identity(marker)]
        observations=defaultdict(list)
        for event in rows:
            fields=event['args']
            if fields.get('interval_kind')=='elapsed_cpu_clock' and event['name'] in ('render_queue','request_to_ui'):
                observations[event['name']].append(fields['duration_ns']/1e6)
            if event['name']=='render_task' and event['dur']>0:
                observations['render_task'].append(event['dur']/1e6)
        receipts=[event['ts'] for event in rows if event['name']=='image_accepted']
        uses=[event['ts'] for event in rows if event['name']=='image_frame_use']
        receipt=min(receipts,default=None);use=min(uses,default=None)
        result.append({'source':source,**dict(zip(('task','revision','photo'),identity(marker))),
                       'queue_wait_ms':distribution(observations['render_queue']),
                       'render_elapsed_ms':distribution(observations['render_task']),
                       'request_to_ui_ms':distribution(observations['request_to_ui']),
                       'receipt_from_action_ms':None if receipt is None else (receipt-begin_ns)/1e6,
                       'native_encoding_from_action_ms':None if use is None else (use-begin_ns)/1e6,
                       'receipt_to_encoding_ms':None if receipt is None or use is None else (use-receipt)/1e6})
    return result


def gpu_query_status(events):
    """Report every pending query, including auxiliary work cut by capture stop."""
    lifecycle=[event for event in events if event['name'].startswith('gpu.query.')]
    key=lambda event:(event['args'].get('capture_id'),event['args'].get('query'))
    started={key(event):event['args'] for event in lifecycle if event['name']=='gpu.query.started'}
    resolved={key(event):event['args'] for event in lifecycle if event['name']=='gpu.query.resolved'}
    failures=[event['args'] for event in lifecycle if event['name'] in ('gpu.query.failed','gpu.query.dropped')]
    pending=started.keys()-resolved.keys()
    purposes={fields.get('purpose') for fields in [*started.values(),*resolved.values(),*failures]}
    by_purpose=[]
    for purpose in sorted(purposes,key=str):
        begins={identity for identity,fields in started.items() if fields.get('purpose')==purpose}
        ends={identity for identity,fields in resolved.items()
              if started.get(identity,fields).get('purpose')==purpose}
        errors=[fields for fields in failures if fields.get('purpose')==purpose]
        by_purpose.append({'purpose':purpose,'started':len(begins),'resolved':len(ends),
                           'unresolved':len(begins-resolved.keys()),'failures':errors,
                           'complete':False if errors else (not (begins-resolved.keys()) if begins else None)})
    return {'started':len(started),'resolved':len(resolved),'unresolved':len(pending),
            'unresolved_queries':[started[identity] for identity in sorted(pending,key=str)],
            'resolved_without_start':[resolved[identity] for identity in sorted(resolved.keys()-started.keys(),key=str)],
            'by_purpose':by_purpose,'failures':failures,
            'complete':False if failures else (not pending if started else None),
            'limits':['Status covers this capture window. Pending auxiliary work remains incomplete, even when current images are ready.',
                      'No additional GPU completion wait is introduced to close the recording.']}


def analyze_tables(tables, capture_summary):
    events = events_from_tables(tables)
    markers = [event for event in events if event['name'] == 'scenario.step']
    grouped = defaultdict(list)
    for event in markers:
        grouped[event['args'].get('step')].append(event)
    steps, failures, incomplete = [], [], []
    for step, rows in sorted(grouped.items(), key=lambda pair: min(row['ts'] for row in pair[1])):
        starts = [row for row in rows if row['args'].get('phase') == 'begin']
        ends = [row for row in rows if row['args'].get('phase') == 'settled']
        if len(starts) != 1 or len(ends) != 1:
            failures.append(f'step {step}: missing or duplicate begin/settled marker')
            failed = [row for row in rows if row['args'].get('phase') == 'failed']
            if len(starts) == 1 and len(failed) == 1:
                begin, end = starts[0], failed[0]
                window = [event for event in events if begin['ts'] <= event['ts'] <= end['ts']]
                incomplete.append({'step': step, 'action': begin['args'].get('action'),
                                   'failure': end['args'], 'interactions': interaction_events(window, begin['ts'])})
            continue
        begin, end = starts[0], ends[0]
        if end['ts'] < begin['ts']:
            failures.append(f'step {step}: settlement precedes its action')
            continue
        window = [event for event in events if begin['ts'] <= event['ts'] <= end['ts']]
        current = [event for event in rows if event['args'].get('phase') == 'current']
        by_source = {event['args'].get('source'): event for event in current}
        expected_count = begin['args'].get('photo_count')
        issues = []
        if None in by_source or len(by_source) != expected_count:
            issues.append('not every expected photo has a current result')
        if any(event['args'].get('expected_scale_divisor') is None
               or event['args'].get('scale_divisor') != event['args']['expected_scale_divisor']
               for event in current):
            issues.append('an effective scale differs from its photo-specific requested scale')
        if end['args'].get('worker_busy') is not False or end['args'].get('progress_running') is not False:
            issues.append('desired work or progress did not settle')
        work, work_issues = expected_work(window, begin['args'], {event['args'].get('photo') for event in current},
                                          {source: event['args'].get('photo') for source, event in by_source.items()})
        issues.extend(work_issues)
        if begin['args'].get('action') == 'background-edit':
            edits = [event for event in window if event['name'] == 'scenario.background_edit']
            overlapping = [event for event in events if event['name'] == 'batch.photo' and event['dur'] > 0
                           and any(event['ts'] <= edit['ts'] < event['ts'] + event['dur'] for edit in edits)]
            work['background_overlap'] = {'edit_markers':len(edits), 'active_batch_photos':len(overlapping)}
            if len(edits) != 1 or not overlapping:
                issues.append('foreground edit did not overlap an actual batch photo operation')
        latencies = [(event['ts'] - begin['ts']) / 1e6 for event in by_source.values()]
        expected_scales = {identity(event): event['args'].get('expected_scale_divisor')
                           for event in by_source.values()}
        identities = set(expected_scales)
        accepted = [event for event in window if event['name'] == 'image_accepted' and
                    identity(event) in identities]
        uses = [event for event in window if event['name'] == 'image_frame_use' and
                identity(event) in identities]
        if any(event['args'].get('scale') != expected_scales[identity(event)] for event in accepted):
            issues.append('accepted artifact scale differs from the requested scale')
        encoded = {identity(event) for event in uses if event['args'].get('scale') == expected_scales[identity(event)]}
        if any(None in value for value in identities) or identities - encoded:
            issues.append('not every current photo identity reached native encoding at the requested scale')
        peaks = defaultdict(list)
        for counter in tables['counters']:
            if begin['ts'] <= int(counter['ts']) <= end['ts']:
                peaks[counter['name']].append(float(counter['value']))
        outcomes = Counter(event['name'] for event in window if event['name'] in
                           ('completed', 'completion_superseded', 'completion_removed', 'admitted'))
        steps.append({**work, 'step': step, 'action': begin['args'].get('action'),
                      'interactions': interaction_events(window, begin['ts']),
                      'coverage': begin['args'].get('action_kind', begin['args'].get('coverage')), 'photo_count': expected_count,
                      'scale_divisors': sorted({value for value in expected_scales.values() if value is not None}),
                      'valid': not issues, 'issues': issues,
                      'first_current_observed_ms': min(latencies, default=None),
                      'all_current_observed_ms': max(latencies, default=None),
                      'group_settled_ms': (end['ts'] - begin['ts']) / 1e6,
                      'photo_latency_ms': distribution(latencies),
                      'first_receipt_ms': min(((event['ts'] - begin['ts']) / 1e6 for event in accepted), default=None),
                      'first_native_encoding_ms': min(((event['ts'] - begin['ts']) / 1e6 for event in uses), default=None),
                      'outcomes': dict(outcomes), 'counter_peaks': {key: max(values) for key, values in peaks.items()},
                      'photo_lifecycle':photo_lifecycle(window,by_source,begin['ts']),
                      'current': [{'source': source, **{key:event['args'].get(key) for key in ('task','revision','photo')}}
                                  for source, event in by_source.items()]})
        failures.extend(f'step {step}: {issue}' for issue in issues)
    smoke = analyze_smoke(events)
    if smoke:
        failures.extend(smoke['failures'])
    validity = loss_status(tables['stats'], capture_summary.get('complete'))
    validity['emission_transition_drops'] = capture_summary.get('emission_transition_drops',0)
    if validity['emission_transition_drops']:
        validity['valid'] = False
    scopes, elapsed, gpu = defaultdict(list), defaultdict(list), []
    for event in events:
        fields = event['args']
        if event['dur'] > 0:
            scopes[(event['category'], event['name'])].append(event['dur'] / 1e6)
        if fields.get('interval_kind') == 'elapsed_cpu_clock':
            elapsed[event['name']].append(fields['duration_ns'] / 1e6)
        if all(key in fields for key in ('clock_id','start_tick','end_tick','duration_ns')):
            gpu.append(fields)
    gpu_status = gpu_query_status(events)
    return {'schema_version': 1, 'input_kind': 'perfetto', 'validity': validity,
            'behavior_valid': (not failures and bool(steps)) if markers else (smoke['valid'] if smoke else None), 'behavior_failures': failures,
            'smoke':smoke,
            'steps': steps, 'incomplete_steps': incomplete,
            'work_census': node_work_census(events),
            'event_counts': dict(Counter(event['name'] for event in events)),
            'flow_count': len(tables['flows']),
            'host_scope_elapsed_ms': [{'category':category,'name':name,**distribution(values)} for (category,name),values in sorted(scopes.items())],
            'elapsed_observations_ms': {name:distribution(values) for name,values in elapsed.items()},
            'gpu_observations': gpu,
            'scope_attribution':scope_attribution(events,tables),
            'gpu_node_submission':gpu_attribution(events),
            'gpu_query_status': gpu_status,
            'incomplete_scope_count': sum(event['dur'] < 0 for event in events),
            'limits': ['Host scopes are elapsed intervals; nested and parallel scopes are not additive.',
                       'Current-observed markers are UI observations, not exact image receipt or screen presentation.',
                       'Native command encoding does not prove physical screen presentation.',
                       'GPU observations retain their own clock; no host position is synthesized.']}


def analyze_capture(directory, tool, attach=True):
    directory = Path(directory)
    summary_path = directory / 'summary.json'
    summary = json.loads(summary_path.read_text())
    tables = query_trace(tool, directory / 'trace.pftrace', directory / 'analysis')
    result = analyze_tables(tables, summary)
    node_rows = result['work_census'].pop('nodes')
    (directory/'analysis'/'node-work.json').write_text(json.dumps(node_rows, indent=2)+'\n')
    result['work_census']['nodes'] = 'analysis/node-work.json'
    attribution=result['scope_attribution']
    hierarchy=attribution.pop('scopes')
    hierarchy_path=directory/'analysis'/'scope-hierarchy.json'
    hierarchy_path.write_text(json.dumps({'measurement':attribution['measurement'],'scopes':hierarchy},indent=2)+'\n')
    attribution.update(hierarchy='analysis/scope-hierarchy.json',scope_count=len(hierarchy),
                       maximum_depth=max((row['depth'] or 0 for row in hierarchy),default=0),
                       thread_track_count=len({row['track'] for row in hierarchy if row['thread']}))
    version = subprocess.check_output([str(tool), '--version'], text=True).strip()
    tool_hash = digest(tool)
    result['trace_processor'] = {'path': str(Path(tool).resolve()), 'version': version, 'sha256': tool_hash}
    if attach:
        summary['loss_checked'] = result['validity']['loss_checked']
        summary['analysis'] = result
        summary_path.write_text(json.dumps(summary, indent=2) + '\n')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--trace-processor', type=Path, required=True)
    args = parser.parse_args()
    result = analyze_capture(args.directory, args.trace_processor)
    print(json.dumps(result, indent=2))
    if not result['validity']['valid'] or result['behavior_valid'] is False:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
