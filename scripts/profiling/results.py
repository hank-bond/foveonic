"""Report shared render observations and preview-scale assertions across test drivers."""

PREVIEW_QUALITIES = ('full', 'fast', 'draft', 'thumb')


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
