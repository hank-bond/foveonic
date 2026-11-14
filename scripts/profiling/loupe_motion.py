"""Check retained loupe coverage and sustained input independently of render scheduling."""

import math


def check_return_coverage(loupe):
    if loupe.get('single_whole') is True:
        return
    names = ('covered loupe withdrew held replacement', 'leaving covered loupe requested fresh detail')
    rows = [event for event in loupe.get('events', []) if event.get('observed') in names]
    if [row.get('observed') for row in rows] != list(names):
        raise ValueError('loupe journey lacks return-to-coverage and fresh-request evidence')
    retained, fresh = rows
    def contains(region, required):
        return (len(region) == len(required) == 4
                and all(region[i] <= required[i] for i in (0, 1))
                and all(region[i] >= required[i] for i in (2, 3)))
    if (not retained.get('detail') or retained['detail'] != fresh.get('previous')
            or not fresh.get('detail') or fresh['detail'] == retained['detail']
            or not contains(retained.get('region', []), retained.get('required', []))
            or not contains(fresh.get('region', []), fresh.get('required', []))
            or contains(retained.get('region', []), fresh.get('required', []))):
        raise ValueError('loupe return did not retain coverage and then replace uncovered detail')


def check_motion(loupe):
    rows = [event for event in loupe.get('events', []) if 'motion' in event]
    if [row.get('motion') for row in rows] != ['cached', 'sweep']:
        raise ValueError('loupe journey lacks cached and replacement motion controls')
    widths = []
    for row in rows:
        samples = row.get('samples', [])
        duration = row.get('duration_seconds', 0)
        if (row.get('coverage') != 'egui-input' or not isinstance(duration, (int, float))
                or not math.isfinite(duration) or duration < 3 or len(samples) < 3):
            raise ValueError('loupe motion did not sustain its input interval')
        previous = -1
        positions = []
        for sample in samples:
            seconds, point = sample.get('seconds'), sample.get('position')
            if (not isinstance(seconds, (int, float)) or not math.isfinite(seconds)
                    or seconds <= previous or not isinstance(point, list) or len(point) != 2
                    or any(not isinstance(v, (int, float)) or not math.isfinite(v) for v in point)):
                raise ValueError('loupe motion samples lack ordered time and finite positions')
            previous = seconds
            positions.append(point[0])
        if samples[0]['seconds'] != 0 or abs(previous - duration) > 1e-6:
            raise ValueError('loupe motion samples do not cover the measured interval')
        width = max(positions) - min(positions)
        if width <= 0:
            raise ValueError('loupe motion never moved')
        widths.append(width)
    if widths[1] < widths[0] * 10:
        raise ValueError('loupe sweep did not exercise a wider path than its cached control')
