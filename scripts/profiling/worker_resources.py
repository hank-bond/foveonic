"""Resolve slider-worker guards and admit finite groups within a child-memory budget."""

FAMILIES = ('classic', 'merrill', 'quattro', 'tiger')


def _positive_integer(value, name):
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise ValueError(f'{name} must be a positive integer')
    return value


def validate_case_memory(case):
    """Validate static family hints independently of selections or an overriding guard."""
    hints = case.get('worker_memory_mb_by_family', {})
    if not isinstance(hints, dict) or hints.keys() - set(FAMILIES):
        raise ValueError('case worker memory hints must map known camera families to positive integers')
    for family, value in hints.items():
        _positive_integer(value, f'case worker memory for {family}')
    return hints


def resolve_worker_memory_mb(fixture, override=None, *, case=None):
    """Use global, then case-family, then fixture guards. Custom fixtures still need a base."""
    hints = validate_case_memory(case or {})
    name = f"worker memory for fixture {fixture.get('name', '<unnamed>')}"
    if override is not None:
        return _positive_integer(override, name)
    base = _positive_integer(fixture.get('worker_memory_mb'), name)
    return hints.get(fixture.get('family'), base)


def validate_admission(workers, budget_mb, reservations_mb):
    """Check the complete queue before staging; the host budget excludes OS/Python headroom."""
    _positive_integer(workers, 'workers')
    _positive_integer(budget_mb, 'guarded-child memory budget')
    for index, reservation in enumerate(reservations_mb):
        _positive_integer(reservation, f'group {index} memory reservation')
        if reservation > budget_mb:
            raise ValueError(f'group {index} memory reservation exceeds guarded-child memory budget')


def select_admissions(pending, active, reservations_mb, workers, budget_mb):
    """First-fit pending indices without mutation, retaining each active group's full guard.

    A worker runs only one guarded child at a time and holds its reservation through
    native execution and sequential verification. Pending is a finite queue: passing
    a currently blocked group cannot starve it once the active groups finish.
    """
    validate_admission(workers, budget_mb, reservations_mb)
    indices = [*pending, *active]
    if (any(isinstance(index, bool) or not isinstance(index, int)
            or index < 0 or index >= len(reservations_mb) for index in indices)
            or len(set(indices)) != len(indices)):
        raise ValueError('pending and active group indices must be valid, unique and disjoint')
    available = budget_mb - sum(reservations_mb[index] for index in active)
    if len(active) > workers or available < 0:
        raise ValueError('active groups exceed worker or guarded-child memory limits')
    selected = []
    for index in pending:
        if len(active) + len(selected) == workers:
            break
        if reservations_mb[index] <= available:
            selected.append(index)
            available -= reservations_mb[index]
    return selected
