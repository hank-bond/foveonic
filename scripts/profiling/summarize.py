"""Summarize native feedback and completed-processing JSON reports, keeping modes and stage domains separate."""

import argparse
from collections import Counter, defaultdict
import json
import math
from pathlib import Path
import statistics


def stats(values):
    values = sorted(values)
    return {'count': len(values), 'median': statistics.median(values) if values else None,
            'p95': values[max(0, math.ceil(.95 * len(values)) - 1)] if values else None,
            'maximum': max(values, default=None)}


def parse_record(record):
    if 'line' in record:
        try:
            report = json.loads(record['line'])
        except json.JSONDecodeError:
            return None
    else:
        report = record
    if not isinstance(report, dict) or report.get('foveonic_profile') != 1:
        return None
    if report.get('mode') not in ['off', 'wall', 'gpu'] or not isinstance(report.get('data'), dict):
        raise ValueError('invalid profiling envelope')
    return {**report['data'], 'mode': report['mode'], 'event': report['event'],
            'external_ns': record.get('monotonic_ns')}


def interaction_windows(events, settle):
    windows = []
    start = None
    for event in events:
        if event['kind'] == 'drag_begin':
            start = event['monotonic_ns']
        elif event['kind'] == 'drag_end' and start is not None:
            windows.append((start, event['monotonic_ns'] + int(settle * 1e9)))
            start = None
    if start is not None:
        raise ValueError('unfinished drag')
    return windows


def summarize(records, pointers=None, settle=1.):
    rows = [row for record in records if (row := parse_record(record)) is not None]
    windows = interaction_windows(pointers, settle) if pointers is not None else None
    if windows is not None and not windows:
        raise ValueError('no completed interaction windows')
    if windows is not None and any(row['external_ns'] is None for row in rows):
        raise ValueError('pointer selection requires capture timestamps')

    def selected(row):
        return windows is None or any(a <= row['external_ns'] <= b for a, b in windows)

    result = {'modes': {}, 'limits': [
        'Host timings are elapsed wall durations, not CPU-active time; nested spans are not additive.',
        'GPU diagnostic resolutions synchronize; do not use their wall times as acceptance measurements.',
        'GPU pass durations exclude initialization and inter-pass waits; residual wall time is not pure queue wait.',
        'UI receipt and callback encoding are not GPU drawing completion or screen scanout.',
        'External timestamps select pointer windows, not exact native-event latency.',
        'Activity byte counts are allocation/transfer totals, not residency.',
        'Missing GPU stages mean unrecorded or skipped work, not zero execution cost.']}
    for mode in sorted({row['mode'] for row in rows}):
        all_rows = [row for row in rows if row['mode'] == mode]
        selected_rows = [row for row in all_rows if selected(row)]
        delivered = {row['revision'] for row in selected_rows if row['event'] == 'image'}
        candidates = [row for row in all_rows if selected(row) or row.get('revision') in delivered]
        summary = {'synchronized': mode == 'gpu', 'delivered_revision_count': len(delivered),
                   'worker_ms': stats([r['worker_ms'] for r in selected_rows if r['event'] == 'image']),
                   'queue_to_ui_ms': stats([r['queue_to_ui_ms'] for r in selected_rows if r['event'] == 'image']),
                   'busy_frame_gap_ms': stats([r['gap_ms'] for r in selected_rows if r['event'] == 'frame' and r.get('busy') and r.get('gap_ms') is not None]),
                   'ui_handling_ms': stats([r['elapsed_ms'] for r in selected_rows if r['event'] == 'ui_update']),
                   'previous_native_frame_cpu_ms': stats([r['previous_cpu_ms'] for r in selected_rows if r['event'] == 'frame' and r.get('previous_cpu_ms') is not None]),
                   'prepared_to_encode_ms': stats([r['prepared_to_encode_ms'] for r in selected_rows if r['event'] == 'draw']),
                   'processing_status': dict(Counter(r.get('status', 'complete') for r in selected_rows if r['event'] == 'processing')),
                   'spans_ms': {}, 'completed_processing': [], 'stages': [],
                   'activity_per_profile': [],
                   'sources': sorted({str(r['source']) for r in candidates if r.get('source') is not None})}
        for phase in sorted({r['phase'] for r in candidates if r['event'] == 'span'}):
            phase_rows = selected_rows if phase in ['draw_prepare', 'draw_encode'] else candidates
            summary['spans_ms'][phase] = stats([r['elapsed_ms'] for r in phase_rows if r['event'] == 'span' and r['phase'] == phase])
        summary['worker_queue_ms'] = stats([r['queue_ms'] for r in candidates if r['event'] == 'lifecycle' and r.get('phase') == 'worker_started' and (windows is None or r.get('revision') in delivered)])
        lifecycle = {(r['revision'], r['phase']): r['at_ms'] for r in all_rows if r['event'] == 'lifecycle'}
        requests = {r['revision']: r['at_ms'] for r in all_rows if r['event'] == 'request'}
        summary['request_to_receipt_ms'] = stats([lifecycle[(rev, 'image_received')] - requests[rev] for rev in delivered if rev in requests and (rev, 'image_received') in lifecycle])
        summary['ready_to_receipt_ms'] = stats([lifecycle[(rev, 'image_received')] - lifecycle[(rev, 'reply_ready')] for rev in delivered if (rev, 'image_received') in lifecycle and (rev, 'reply_ready') in lifecycle])
        first_encode = {}
        for row in all_rows:
            if row['event'] == 'span' and row['phase'] == 'draw_encode':
                first_encode.setdefault(row['revision'], row['end_ms'])
        summary['receipt_to_first_encoding_end_ms'] = stats([first_encode[rev] - lifecycle[(rev, 'image_received')] for rev in delivered if rev in first_encode and (rev, 'image_received') in lifecycle])
        # Fresh renders and slider feedback measure different work. Mixing them,
        # or mixing cold and warm runs, would hide the cost the report is meant to show.
        for execution in sorted({r.get('execution') for r in selected_rows if r['event'] == 'processing' and r.get('completed_ms') is not None}):
            for cold in [True, False]:
                values = [r['completed_ms'] for r in selected_rows if r['event'] == 'processing' and r.get('execution') == execution and bool(r.get('cold')) == cold and r.get('completed_ms') is not None]
                if values:
                    summary['completed_processing'].append({'execution': execution, 'cold': cold, 'milliseconds': stats(values)})
        # Cancelled work did not deliver a frame. Including it would make stage
        # costs look cheaper than the work needed to show a completed image.
        profile_rows = [r for r in candidates if 'profile' in r and r.get('status') != 'superseded'
                        and (windows is None or r.get('revision') in delivered)]
        for phase in sorted({r['event'] for r in profile_rows if 'activity' in r}):
            phase_rows = [r for r in profile_rows if r['event'] == phase and 'activity' in r]
            fields = sorted({field for r in phase_rows for field in r['activity']})
            summary['activity_per_profile'].append({'phase': phase, 'counts': {
                field: stats([r['activity'][field] for r in phase_rows if field in r['activity']])
                for field in fields}})
        groups = defaultdict(lambda: {'passes': [], 'totals': [], 'counts': []})
        for row in profile_rows:
            for domain in ['host', 'gpu']:
                per_report = defaultdict(list)
                for sample in row['profile'].get(domain, []):
                    per_report[sample['stage']].append(sample['milliseconds'])
                for stage, values in per_report.items():
                    group = groups[(row['event'], domain, stage)]
                    group['passes'].extend(values)
                    group['totals'].append(sum(values))
                    group['counts'].append(len(values))
        for (phase, domain, stage), group in sorted(groups.items()):
            summary['stages'].append({'phase': phase, 'domain': domain, 'stage': stage,
                                     'sample_ms': stats(group['passes']),
                                     'per_profile_total_ms': stats(group['totals']),
                                     'samples_per_profile': stats(group['counts'])})
        result['modes'][mode] = summary
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('records', type=Path, help='native stderr.jsonl or completed-processing stdout JSONL')
    parser.add_argument('--pointers', type=Path)
    parser.add_argument('--settle', type=float, default=1.)
    args = parser.parse_args()
    if args.settle < 0 or not math.isfinite(args.settle):
        parser.error('settlement window must be finite and nonnegative')
    records = [json.loads(line) for line in args.records.read_text().splitlines() if line.strip()]
    pointers = [json.loads(line) for line in args.pointers.read_text().splitlines()] if args.pointers else None
    print(json.dumps(summarize(records, pointers, args.settle), indent=2))


if __name__ == '__main__':
    main()
