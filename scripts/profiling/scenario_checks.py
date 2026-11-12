"""Check declared scenario actions and intended edits independently of image replay."""

import json
import struct


def pointer_value(settings, pointer):
    value = settings
    for token in pointer.removeprefix('/').split('/'):
        token = token.replace('~1', '/').replace('~0', '~')
        value = value[int(token)] if isinstance(value, list) else value[token]
    return value


def same_numeric(left, right):
    if type(left) not in (int, float) or type(right) not in (int, float):
        return False
    if type(left) is int and type(right) is int:
        return left == right
    return struct.pack('f', left) == struct.pack('f', right)


def validate_choice_recipes(cases):
    """Require a mixed choice and an enabled, visibly non-neutral Tone adjustment."""
    balances = []
    for case in cases:
        settings = case.get('settings')
        if not isinstance(settings, dict):
            raise ValueError('choice-journey needs explicit recipes')
        balance = settings.get('white_balance')
        tone = settings.get('tone')
        if (balance not in ('Sunlight', 'Incandescent') or settings.get('tone_enabled') is not True
                or not isinstance(tone, dict) or type(tone.get('contrast')) not in (int, float)
                or tone['contrast'] != 0.25):
            raise ValueError('choice-journey needs Sunlight/Incandescent and enabled Tone with contrast 0.25')
        balances.append(balance)
    if len(balances) < 2 or set(balances) != {'Sunlight', 'Incandescent'}:
        raise ValueError('choice-journey needs mixed Sunlight and Incandescent recipes')


def check_slider_drag(native_result, initial, photos, declared, directory, pointer):
    drag = json.loads((directory/'evidence'/'slider-drag.json').read_text())
    paths, starts, frames = drag['paths'], drag['initial'], drag['frames']
    if (not native_result.get('passed') or drag['pointer'] != pointer or drag['errors']
            or len(paths) != len(declared) or len(set(paths)) != len(declared)
            or set(paths) != set(declared) or len(starts) != len(paths) or not frames
            or len(frames[-1]['values']) != len(paths)):
        raise ValueError('slider drag lacks checked pointer movement evidence')
    for source, start, final in zip(paths, starts, frames[-1]['values']):
        if (not same_numeric(start, pointer_value(initial[source], pointer))
                or not same_numeric(final, pointer_value(photos[source]['settings'], pointer))):
            raise ValueError('slider drag values differ from the checked initial or final settings')


def check_exposures(expected, inputs, photos):
    expected_exposures = expected['expected']['exposures']
    if (len(expected_exposures) != len(inputs)
            or any(photos[item['staged_source']]['settings']['exposure'] != exposure
                   for item,exposure in zip(inputs,expected_exposures))):
        raise ValueError('journey lacks the expected per-photo edits')


def check_choices(choices, inputs, initial, photos, declared):
    validate_choice_recipes(inputs)
    observed = choices.get('events', [])
    if (not any(event.get('observation') == 'mixed White Balance' for event in observed)
            or not any(event.get('action') == 'choose Sunlight' for event in observed)
            or not any(event.get('action') == 'disable Tone' for event in observed)
            or not any(event.get('observation') == 'Tone unchecked and final images current' for event in observed)):
        raise ValueError('choice journey lacks checked dropdown and checkbox input evidence')
    for source, record in declared.items():
        start, final, recipe = initial[source], photos[source]['settings'], record['settings']
        if (start.get('white_balance') != recipe['white_balance']
                or start.get('tone_enabled') is not True
                or start.get('tone', {}).get('contrast') != 0.25):
            raise ValueError('initial choices do not match the declared recipe')
        if (final.get('white_balance') != 'Sunlight' or final.get('tone_enabled') is not False
                or final.get('tone', {}).get('contrast') != 0.25
                or not same_numeric(final.get('exposure'), recipe.get('exposure'))):
            raise ValueError('final choices or exposure differ from the independent expectation')


def check_transitions(transitions, inputs, photos):
    completed = [event['completed'] for event in transitions['events'] if 'completed' in event]
    if not completed or completed != transitions['sequence']:
        raise ValueError('transition journey lacks its complete replay sequence')
    composed = transitions.get('composition')
    if not isinstance(composed, list) or len(composed) != 3 or not 0 <= composed[0] < len(inputs):
        raise ValueError('transition journey lacks its expected composition')
    settings = photos[inputs[composed[0]]['staged_source']]['settings']
    if settings.get('crop') != composed[1] or settings.get('rotation') != composed[2]:
        raise ValueError('transition composition differs from the independent expectation')


def check_selection_delivery(journey):
    if (not any(event.get('observation') == 'delivery_held' for event in journey['events'])
            or not any(event.get('action') == 'release_delivery' for event in journey['events'])):
        raise ValueError('selection journey lacks controlled delivery evidence')


def check_captured_settings(photo, effective, pointer, choices):
    if not same_numeric(pointer_value(effective, pointer), pointer_value(photo['settings'], pointer)):
        raise ValueError('captured image settings do not match the checked edited property and scale')
    if choices and (effective.get('white_balance') != 'Sunlight'
                    or effective.get('tone_enabled') is not False
                    or effective.get('tone', {}).get('contrast') != 0.25):
        raise ValueError('captured image choices differ from the independent expectation')
    if pointer == '/exposure' and (effective['exposure_auto'] or photo['settings']['exposure_auto']):
        raise ValueError('captured image settings do not match the checked manual exposure')
    for field in ('crop', 'rotation', 'saturation'):
        if field in photo['settings'] and effective.get(field) != photo['settings'][field]:
            raise ValueError(f'captured image {field} differs from the checked authored setting')


def checked_scenario_images(native_result, inputs, directory, control_pointer='/exposure'):
    """Return the checked final action and photos before replaying their captured pixels."""
    journey = native_result.get('journey')
    background = native_result.get('background')
    transitions = native_result.get('transitions')
    lifecycle = native_result.get('lifecycle')
    browser = native_result.get('browser_journey')
    choices = native_result.get('choices')
    final_action = ('restore-selection' if journey else 'background-finish' if background else
                    'transition-journey' if transitions else 'lifecycle-journey' if lifecycle else
                    'browser-journey' if browser else 'choice-journey' if choices else 'slider-drag')
    steps = [step for step in native_result['steps'] if step['action'] == final_action]
    initial_steps = [step for step in native_result['steps'] if step['action'] == 'initial-selection']
    captures = native_result.get('image_captures', [])
    if len(steps) != 1 or len(initial_steps) != 1 or len(captures) != len(inputs):
        raise ValueError('missing initial/final slider step or image captures')
    photos = {photo['source']:photo for photo in steps[0]['photos']}
    initial = {photo['source']:photo['settings'] for photo in initial_steps[0]['photos']}
    declared = {item['staged_source']:item for item in inputs}
    if len(photos) != len(inputs) or set(photos) != set(declared) or set(initial) != set(declared):
        raise ValueError('final slider photos do not match the declared inputs')
    is_journey = bool(journey or background or transitions or lifecycle or browser or choices)
    pointer = '/exposure' if is_journey else control_pointer
    for source, record in declared.items():
        expected = pointer_value(record['settings'], pointer)
        if not same_numeric(pointer_value(initial[source], pointer), expected):
            raise ValueError(f'initial {pointer} does not match the declared recipe')
        if pointer == '/exposure' and initial[source]['exposure_auto']:
            raise ValueError('initial exposure does not match the declared manual recipe')
    if not is_journey:
        check_slider_drag(native_result, initial, photos, declared, directory, pointer)
    if journey or background or transitions or lifecycle or browser:
        check_exposures(journey or background or transitions or lifecycle or browser, inputs, photos)
    if choices:
        check_choices(choices, inputs, initial, photos, declared)
    if transitions:
        check_transitions(transitions, inputs, photos)
    if journey:
        check_selection_delivery(journey)
    for capture in captures:
        source = capture['source']
        if source not in photos:
            raise ValueError('an image capture references an undeclared photo')
        check_captured_settings(photos[source], capture['settings']['settings'], pointer, choices)
    return final_action, photos
