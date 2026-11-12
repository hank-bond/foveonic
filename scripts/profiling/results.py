"""Report shared preview-scale assertions and exact image comparisons across test drivers."""

import hashlib

PREVIEW_QUALITIES = ('full', 'fast', 'draft', 'thumb')
IMAGE_GEOMETRY_FIELDS = ('extent', 'layout', 'space', 'linear_white', 'view_baked',
                         'view_extent', 'view_crop', 'view_rotation', 'endianness')


# This is the workspace preview contract, kept independent of either renderer.
# Tiger uses the Quattro processing family for this policy.
def expected_preview_scale(quality, family):
    if quality not in PREVIEW_QUALITIES:
        raise ValueError(f'unknown preview quality: {quality}')
    if family not in ('Classic', 'Merrill', 'Quattro'):
        return None
    return {'full': 1, 'fast': 2, 'draft': 4,
            'thumb': 4 if family == 'Classic' else 8}[quality]


def render_result(driver, scenario, quality, observations, execution_passed, failure=None, environment=None):
    """Check each delivered image against the preview policy, retaining all observations."""
    if quality not in PREVIEW_QUALITIES:
        raise ValueError(f'unknown preview quality: {quality}')
    assertions = []
    for observation in observations:
        expected = expected_preview_scale(quality, observation.get('family'))
        actual = observation.get('scale_divisor')
        if expected is None or type(actual) is not int:
            outcome = 'unverified'
        else:
            outcome = 'passed' if actual == expected else 'failed'
        assertions.append({'name': 'preview_scale', 'step': observation['step'],
                           'source': observation['source'], 'expected': expected,
                           'actual': actual, 'outcome': outcome})
    outcomes = {assertion['outcome'] for assertion in assertions}
    if not execution_passed:
        outcome = 'failed'
        if failure is None:
            failure = 'the test driver failed'
    elif 'failed' in outcomes:
        outcome = 'failed'
        if failure is None:
            failure = 'a preview-scale assertion failed'
    elif not observations or 'unverified' in outcomes:
        outcome = 'incomplete'
        if failure is None:
            failure = 'preview-scale evidence is incomplete'
    else:
        outcome = 'passed'
    return {'schema_version': 1, 'driver': driver, 'scenario': scenario,
            'quality': quality, 'environment': environment or {}, 'outcome': outcome, 'failure': failure,
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
