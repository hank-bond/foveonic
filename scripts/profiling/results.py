"""Report shared preview-scale assertions and exact image comparisons across test drivers."""

import hashlib
import math
import struct

RESOLUTIONS = ('full', 'fast', 'draft', 'thumb')
PROCESSING_QUALITIES = ('quality', 'speed')
IMAGE_GEOMETRY_FIELDS = ('extent', 'layout', 'space', 'linear_white', 'view_baked',
                         'view_extent', 'view_crop', 'view_rotation', 'endianness')


def _f32(value):
    """Match the app's geometry precision at arithmetic boundaries."""
    return struct.unpack('<f', struct.pack('<f', value))[0]


# Numeric resolution probes are distinct from the application's automatic sizing.
def expected_resolution_scale(resolution, family):
    if resolution not in RESOLUTIONS:
        raise ValueError(f'unknown resolution: {resolution}')
    if family not in ('Classic', 'Merrill', 'Quattro'):
        return None
    return {'full': 1, 'fast': 2, 'draft': 4,
            'thumb': 4 if family == 'Classic' else 8}[resolution]


def expected_display_scale(processing_quality, family, density):
    """Independent display-demand contract. Finer completed frames remain sufficient."""
    if processing_quality not in PROCESSING_QUALITIES:
        raise ValueError(f'unknown processing quality: {processing_quality}')
    if family not in ('Classic', 'Merrill', 'Quattro') or not isinstance(density, (float, int)) or not math.isfinite(density) or density <= 0:
        return None
    density = _f32(density)
    limit = _f32(1.1 if processing_quality == 'quality' else 1.2)
    for divisor in (8, 4, 2, 1):
        if _f32(density * divisor) <= limit or divisor == 1:
            return min(divisor, 4) if family == 'Classic' else divisor


def observation_density(observation):
    """Compute physical sampling from raw drawn geometry when available."""
    size, extent, dpr = (observation.get(key) for key in ('display_size_points', 'native_extent', 'pixels_per_point'))
    if size is not None and extent is not None and dpr is not None:
        if (not isinstance(size, (list, tuple)) or not isinstance(extent, (list, tuple))
                or len(size) != 2 or len(extent) != 2
                or any(not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0 for value in (*size, *extent, dpr))):
            return None
        return _f32(max(_f32(_f32(size[axis]) / _f32(extent[axis])) for axis in (0, 1)) * _f32(dpr))
    return observation.get('display_density')


def render_result(driver, scenario, processing_quality, observations, execution_passed, failure=None, environment=None, resolution=None):
    """Check selected processing mode and numeric or automatic working resolution."""
    if processing_quality not in PROCESSING_QUALITIES:
        raise ValueError(f'unknown processing quality: {processing_quality}')
    if resolution is not None and resolution not in RESOLUTIONS:
        raise ValueError(f'unknown resolution: {resolution}')
    assertions = []
    for observation in observations:
        selected = observation.get('requested_processing_quality', processing_quality)
        actual_quality = observation.get('processing_quality')
        if selected not in PROCESSING_QUALITIES or actual_quality not in PROCESSING_QUALITIES:
            quality_outcome = 'unverified'
        else:
            quality_outcome = 'passed' if selected == actual_quality else 'failed'
        expected = (expected_resolution_scale(resolution, observation.get('family')) if resolution is not None
                    else expected_display_scale(selected, observation.get('family'), observation_density(observation)) if selected in PROCESSING_QUALITIES else None)
        actual = observation.get('scale_divisor')
        if expected is None or type(actual) is not int:
            outcome = 'unverified'
        else:
            sufficient = actual == expected if resolution is not None else actual in (1, 2, 4, 8) and actual <= expected
            outcome = 'passed' if sufficient else 'failed'
        assertions.extend([
            {'name':'preview_scale', 'step':observation['step'], 'source':observation['source'],
             'expected':expected, 'actual':actual, 'outcome':outcome},
            {'name':'processing_quality', 'step':observation['step'], 'source':observation['source'],
             'expected':selected, 'actual':actual_quality, 'outcome':quality_outcome},
        ])
    outcomes = {assertion['outcome'] for assertion in assertions}
    if not execution_passed:
        outcome = 'failed'
        if failure is None:
            failure = 'the test driver failed'
    elif 'failed' in outcomes:
        outcome = 'failed'
        if failure is None:
            failure = 'a processing-mode or preview-scale assertion failed'
    elif not observations or 'unverified' in outcomes:
        outcome = 'incomplete'
        if failure is None:
            failure = 'processing-mode or preview-scale evidence is incomplete'
    else:
        outcome = 'passed'
    return {'schema_version': 2, 'driver': driver, 'scenario': scenario,
            'processing_quality': processing_quality, 'resolution': resolution,
            'environment': environment or {}, 'outcome': outcome, 'failure': failure,
            'observations': observations, 'assertions': assertions}


def compare_images(source, step, expected_file, expected_geometry, actual_file, actual_geometry):
    """Compare complete floating image storage and its declared view, without loading it all at once."""
    expected_hash, actual_hash = hashlib.sha256(), hashlib.sha256()
    expected_length = actual_length = 0
    first_difference = None
    with expected_file.open('rb') as expected, actual_file.open('rb') as actual:
        while True:
            left, right = expected.read(65536), actual.read(65536)
            if not left and not right:
                break
            expected_hash.update(left)
            actual_hash.update(right)
            if first_difference is None and left != right:
                first_difference = expected_length + next(
                    (index for index, (a, b) in enumerate(zip(left, right)) if a != b),
                    min(len(left), len(right)))
            expected_length += len(left)
            actual_length += len(right)
    geometry_differs = expected_geometry != actual_geometry
    missing_geometry = [key for key in IMAGE_GEOMETRY_FIELDS
                        if key not in expected_geometry or key not in actual_geometry]
    extent, layout = expected_geometry.get('extent'), expected_geometry.get('layout')
    pixel_size = {'Rgb32Float':12, 'Scalar32Float':4}.get(layout)
    required_length = (extent[0] * extent[1] * pixel_size
                       if pixel_size and isinstance(extent, list) and len(extent) == 2 else None)
    passed = (not geometry_differs and not missing_geometry and required_length is not None
              and expected_length == actual_length == required_length and first_difference is None)
    return {'name':'image_samples', 'source':source, 'step':step,
            'expected':{'file':str(expected_file), 'geometry':expected_geometry, 'bytes':expected_length, 'sha256':expected_hash.hexdigest()},
            'actual':{'file':str(actual_file), 'geometry':actual_geometry, 'bytes':actual_length, 'sha256':actual_hash.hexdigest()},
            'outcome':'passed' if passed else 'failed',
            'difference':{'geometry':geometry_differs, 'missing_geometry':missing_geometry, 'first_byte':first_difference,
                          'required_bytes':required_length}}


def include_assertions(result, assertions):
    """Add shared checks without concealing an earlier driver or assertion failure."""
    result['assertions'].extend(assertions)
    if any(row['outcome'] == 'failed' for row in assertions):
        if result['outcome'] != 'failed':
            failed = next(row for row in assertions if row['outcome'] == 'failed')
            result['failure'] = failed.get('failure') or f"{failed['name']} assertion failed"
        result['outcome'] = 'failed'
    elif any(row['outcome'] == 'unverified' for row in assertions) and result['outcome'] == 'passed':
        result['outcome'] = 'incomplete'
        unverified = next(row for row in assertions if row['outcome'] == 'unverified')
        result['failure'] = f"{unverified['name']} evidence is incomplete"
