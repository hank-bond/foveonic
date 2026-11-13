"""Check and report the two-photo native-detail cache workflow."""

import math
import struct


def same_numeric(left, right):
    if type(left) not in (int, float) or type(right) not in (int, float):
        return False
    return math.isfinite(left) and math.isfinite(right) and struct.pack('f', left) == struct.pack('f', right)


CACHE_WORKFLOW_DELTAS = (0.125, 0.125, -0.125, 0.25)


def check_cache_workflow(native_result, inputs):
    """Check declared actions and authored results; keep observation and admission timings distinct."""
    workflow = native_result.get('cache_workflow')
    steps = native_result.get('steps', [])
    initial = [step for step in steps if step.get('action') == 'initial-selection']
    final = [step for step in steps if step.get('action') == 'cache-workflow']
    if not native_result.get('passed') or not isinstance(workflow, dict) or len(inputs) != 2 or len(initial) != 1 or len(final) != 1:
        raise ValueError('cache workflow lacks two-photo initial and final evidence')
    declared = {item['staged_source']: item for item in inputs}
    starts = {photo['source']: photo['settings'] for photo in initial[0]['photos']}
    photos = {photo['source']: photo for photo in final[0]['photos']}
    if len(declared) != 2 or set(starts) != set(declared) or set(photos) != set(declared):
        raise ValueError('cache workflow photos do not match declared inputs')
    initial_exposures = []
    for item in inputs:
        source = item['staged_source']
        recipe = item['settings']
        exposure = recipe.get('exposure')
        if (type(exposure) not in (int, float) or recipe.get('exposure_auto') is not False
                or starts[source].get('exposure_auto') is not False
                or not same_numeric(starts[source].get('exposure'), exposure)):
            raise ValueError('cache workflow initial exposure differs from the declared manual recipe')
        initial_exposures.append(struct.unpack('f', struct.pack('f', exposure))[0])
    rows = workflow.get('rows')
    if not isinstance(rows, list):
        raise ValueError('cache workflow lacks action evidence')
    actions = [row for row in rows if row.get('event') == 'action']
    names = ['fit-smart', 'zoom-a-200', *[f'exposure-{i}' for i in range(4)], 'return-fit']
    if [row.get('action') for row in actions] != names:
        raise ValueError('cache workflow action sequence differs from the declared actions')
    # Each row belongs to the most recent action and uses the same monotonic clock.
    active = None
    last_time = -1.
    for row in rows:
        timestamp = row.get('elapsed_ms')
        if type(timestamp) not in (int, float) or not math.isfinite(timestamp) or timestamp < last_time:
            raise ValueError('cache workflow has missing or nonmonotonic timing')
        last_time = timestamp
        if row.get('event') == 'action':
            active = row
        else:
            elapsed = row.get('action_elapsed_ms')
            if (active is None or row.get('action') != active['action']
                    or type(elapsed) not in (int, float) or not math.isfinite(elapsed) or elapsed < 0
                    or abs(elapsed - (timestamp - active['elapsed_ms'])) > 1.):
                raise ValueError('cache workflow event is outside its action timeline')
    fit = [row for row in rows if row.get('event') == 'fit-overview']
    if len(fit) != 2 or {row.get('photo') for row in fit} != {0, 1}:
        raise ValueError('cache workflow lacks two Fit demand observations')
    demanded = [None, None]
    for row in fit:
        index = row['photo']
        scale = row.get('demanded_scale')
        if (row.get('action') != 'fit-smart' or row.get('source') != inputs[index]['staged_source']
                or row.get('rendition') != 'Current' or type(scale) is not int or scale < 1
                or type(row.get('effective_scale')) is not int or row['effective_scale'] > scale
                or not same_numeric(row.get('exposure'), initial_exposures[index])):
            raise ValueError('cache workflow Fit evidence does not match its demand')
        demanded[index] = scale
    if len([row for row in rows if row.get('action') == 'return-fit' and row.get('event') == 'fit-current']) != 1:
        raise ValueError('cache workflow lacks return-to-Fit settlement')
    for row, delta in zip(actions[2:6], CACHE_WORKFLOW_DELTAS):
        if not same_numeric(row.get('delta'), delta):
            raise ValueError('cache workflow exposure action differs from the declared delta')
    expected = initial_exposures[:]
    cumulative = []
    for delta in CACHE_WORKFLOW_DELTAS:
        expected = [struct.unpack('f', struct.pack('f', value + delta))[0] for value in expected]
        cumulative.append(expected[:])
    for item, exposure in zip(inputs, expected):
        final_settings = photos[item['staged_source']]['settings']
        if final_settings.get('exposure_auto') is not False or not same_numeric(final_settings.get('exposure'), exposure):
            raise ValueError('cache workflow final exposure differs from the independent expectation')
    timings = []
    for index, action in enumerate(names[1:6]):
        events = [row for row in rows if row.get('action') == action and row.get('event') != 'action']
        for row in events:
            if row.get('event') not in ('first-current', 'first-current-observed', 'reuse-observed'):
                continue
            photo = row.get('photo')
            target = initial_exposures if index == 0 else cumulative[index - 1]
            if (type(photo) is not int or not 0 <= photo < 2
                    or row.get('source') != inputs[photo]['staged_source']
                    or row.get('rendition') != 'Current'
                    or (row.get('event') == 'first-current' and row.get('current') is not True)
                    or not same_numeric(row.get('exposure'), target[photo])):
                raise ValueError('cache workflow current observation has the wrong photo or exposure')
            scale = row.get('effective_scale')
            if type(scale) is not int or scale < 1 or scale > (1 if photo == 0 else demanded[photo]):
                raise ValueError('cache workflow current observation has insufficient scale')
            if photo == 0:
                visible, available = row.get('visible_region'), row.get('available_region')
                if (not isinstance(visible, list) or len(visible) != 4
                        or not isinstance(available, list) or len(available) != 4
                        or any(type(v) is not int for v in visible + available)
                        or visible[0] >= visible[2] or visible[1] >= visible[3]
                        or available[0] > visible[0] or available[1] > visible[1]
                        or available[2] < visible[2] or available[3] < visible[3]):
                    raise ValueError('cache workflow native observation does not cover the viewport')
        current = [row for row in events if row.get('event') in ('first-current', 'first-current-observed', 'reuse-observed')]
        if len(current) != 2 or {row.get('photo') for row in current} != {0, 1}:
            raise ValueError('cache workflow lacks a current observation for each photo')
        whole = [row for row in events if row.get('event') == 'whole-settled']
        if len(whole) != 1:
            raise ValueError('cache workflow lacks whole settlement')
        target = initial_exposures if index == 0 else cumulative[index - 1]
        settled = whole[0]
        if (settled.get('photo') != 0 or settled.get('source') != inputs[0]['staged_source']
                or settled.get('rendition') != 'Current' or settled.get('actual_region') is not None
                or settled.get('effective_scale') != 1 or not same_numeric(settled.get('exposure'), target[0])
                or settled['action_elapsed_ms'] < max(row['action_elapsed_ms'] for row in current)):
            raise ValueError('cache workflow Whole settlement is stale or incomplete')
        foreground = [row for row in events if row.get('event') in ('all-foreground-ready', 'all-foreground-observed')]
        if index and (len(foreground) != 1 or foreground[0]['action_elapsed_ms'] < max(row['action_elapsed_ms'] for row in current)
                      or foreground[0]['action_elapsed_ms'] > settled['action_elapsed_ms']):
            raise ValueError('cache workflow lacks foreground completion')
        def values(event):
            return [row.get('action_elapsed_ms') for row in events if row.get('event') == event]
        timings.append({'action': action, 'evidence_rows':events,
            'first_current_admissions': [row for row in current if row['event'] == 'first-current'],
            'first_current_observations': [row for row in current if row['event'] != 'first-current'],
            'foreground_on_admission_ms': values('all-foreground-ready'),
            'foreground_by_observation_ms': values('all-foreground-observed'),
            'whole_current_admissions': [row for row in events if row.get('event') == 'whole-current-accepted'],
            'whole_settled_observed_ms': whole[0].get('action_elapsed_ms'),
            'regional_admitted_ms': values('regional-accepted'),
            'regional_observed_ms': values('regional-observed')})
    return {'expected_final_exposures': expected, 'actions': timings,
            'limits': ['Observations may describe retained frames; admissions are separate.',
                       'A group completion on admission can include an earlier retained-frame observation.',
                       'Whole settlement is an application observation, not physical screen presentation.']}



def associate_trace(workflow, analysis):
    """Associate current-time trace records; old retained tasks do not claim earlier work."""
    origin = workflow.get('trace_origin_ns')
    if type(origin) is not int or origin <= 0:
        return {'available':False, 'reason':'missing shared trace clock'}
    rows = workflow['rows']
    actions = [row for row in rows if row.get('event') == 'action']
    finish = [row for row in rows if row.get('action') == 'return-fit' and row.get('event') == 'fit-current']
    if len(finish) != 1 or type(finish[0].get('elapsed_ms')) not in (int, float) or not math.isfinite(finish[0]['elapsed_ms']):
        return {'available':False, 'reason':'missing return-to-Fit boundary'}
    snapshots = analysis.get('memory_snapshots', {}).get('pairs', [])
    events = analysis.get('resource_events', [])
    report = []
    for index, action in enumerate(actions):
        start = origin + round(action['elapsed_ms'] * 1e6)
        end = (origin + round(actions[index+1]['elapsed_ms'] * 1e6)
               if index + 1 < len(actions) else origin + round(finish[0]['elapsed_ms'] * 1e6))
        window = lambda ts: start <= ts < end
        admitted = [row for row in rows if row.get('action') == action['action']
                    and row.get('event') in ('first-current', 'regional-accepted', 'whole-current-accepted')
                    and type(row.get('task')) is int and type(row.get('revision')) is int
                    and row['revision'] >= action.get('minimum_revision', 0)]
        tasks = {(row['task'], row['revision']) for row in admitted}
        paired = [pair for pair in snapshots if window(pair['before']['ts_ns'])
                  and window(pair['after']['ts_ns'])
                  and (pair['before'].get('task'), pair['before'].get('revision')) in tasks]
        counts, decisions, reads = {}, {}, {}
        for event in events:
            if not window(event['ts_ns']):
                continue
            name, args = event['name'], event['args']
            counts[name] = counts.get(name, 0) + 1
            if name == 'cache.native_source':
                decision = f"{args.get('decision', 'unknown')}:{args.get('reason', 'unknown')}"
                decisions[decision] = decisions.get(decision, 0) + 1
            if name == 'source.read.complete':
                storage = args.get('storage', 'unknown')
                item = reads.setdefault(storage, {'events':0, 'bytes':0, 'errors':0})
                item['events'] += 1
                item['bytes'] += args.get('bytes', 0)
                item['errors'] += args.get('read_errors', 0)
        report.append({'action':action['action'], 'start_ns':start, 'end_ns':end,
                       'admitted_tasks':[{'task':task,'revision':revision} for task,revision in sorted(tasks)],
                       'matched_memory_pairs':paired, 'window_event_counts':counts,
                       'window_native_decisions':decisions, 'window_source_reads':reads})
    return {'available':True, 'actions':report,
            'limits':['Memory pairs match current action time and newly admitted task/revision; retained older tasks are excluded.',
                      'Window event counts include all work in the interval, including background work.',
                      'Read bytes are assigned by read-completion timestamp; reads can span action boundaries.',
                      'Dispatch counts measure command encoding, not GPU completion.',
                      'Frame resource sizes are logical references that may alias and are not additive.']}


def latency_samples(workflow):
    """Extract finite per-photo and group milestones from an already checked report."""
    actions = workflow.get('actions') if isinstance(workflow, dict) else None
    expected = ['zoom-a-200', *(f'exposure-{index}' for index in range(4))]
    if not isinstance(actions, list) or [row.get('action') for row in actions] != expected:
        raise ValueError('cache workflow requires all five latency actions')

    def elapsed(value):
        if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
            raise ValueError('cache workflow milestone needs a finite nonnegative duration')
        return value

    samples = {}
    for action in actions:
        name = action['action']
        admitted = action.get('first_current_admissions')
        observed = action.get('first_current_observations')
        if not isinstance(admitted, list) or not isinstance(observed, list):
            raise ValueError('cache workflow lacks per-photo current milestones')
        rows = admitted + observed
        if len(rows) != 2 or {row.get('photo') for row in rows} != {0, 1}:
            raise ValueError('cache workflow needs one current milestone per photo')
        photo_times = [elapsed(next(row.get('action_elapsed_ms') for row in rows if row['photo'] == photo))
                       for photo in (0, 1)]
        for photo, value in enumerate(photo_times):
            samples[f'{name}/photo-{photo}-current'] = value
        ready = action.get('foreground_on_admission_ms')
        polled = action.get('foreground_by_observation_ms')
        if not isinstance(ready, list) or not isinstance(polled, list):
            raise ValueError('cache workflow lacks foreground milestone evidence')
        group = ready + polled
        if name == 'zoom-a-200' and not group:
            foreground = max(photo_times)
        elif len(group) == 1:
            foreground = elapsed(group[0])
        else:
            raise ValueError('cache workflow needs one foreground milestone')
        if foreground < max(photo_times):
            raise ValueError('cache workflow foreground precedes its photo milestones')
        whole = elapsed(action.get('whole_settled_observed_ms'))
        if whole < foreground:
            raise ValueError('cache workflow Whole settlement precedes foreground')
        samples[f'{name}/foreground'] = foreground
        samples[f'{name}/whole-observed'] = whole
    return samples
