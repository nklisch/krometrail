"""Configurable range validators.

Creates validator functions from a specification, allowing dynamic
validation rules to be defined at runtime and applied later.
"""


def make_validators(ranges: list[tuple[str, float, float]]) -> list[tuple[str, callable]]:
    """Create named validator functions for numeric ranges.

    Each validator checks whether a value falls within [low, high] inclusive.

    Args:
        ranges: List of (name, low, high) tuples defining each validator.

    Returns:
        List of (name, validator_fn) tuples. Each validator_fn takes a
        single numeric value and returns True if it's within range.

    Example:
        validators = make_validators([
            ("temperature", 36.0, 38.0),
            ("heart_rate", 60.0, 100.0),
        ])
        name, check = validators[0]
        assert check(37.0) is True
    """
    validators = []
    for name, low, high in ranges:
        # BUG: Python closures capture variables by reference, not by value.
        # All closures share the same `low` and `high` variables, which
        # will hold the values from the LAST iteration when called.
        def validate(value):
            return low <= value <= high
        validators.append((name, validate))
    return validators


def validate_reading(validators: list[tuple[str, callable]], readings: dict[str, float]) -> dict[str, bool]:
    """Validate a set of named readings against their respective validators.

    Args:
        validators: Output from make_validators.
        readings: Dict mapping validator name to a numeric value.

    Returns:
        Dict mapping validator name to True (in range) or False (out of range).
    """
    results = {}
    for name, check in validators:
        if name in readings:
            results[name] = check(readings[name])
    return results


def check_patient_vitals(readings: dict[str, float]) -> dict:
    """Check patient vital signs against standard medical ranges.

    Returns a dict with "valid" (bool), "results" (per-vital pass/fail),
    and "out_of_range" (list of vital names that failed).
    """
    vital_ranges = [
        ("temperature", 36.1, 37.2),
        ("heart_rate", 60.0, 100.0),
        ("blood_pressure_sys", 90.0, 140.0),
        ("blood_pressure_dia", 60.0, 90.0),
        ("oxygen_saturation", 95.0, 100.0),
    ]
    validators = make_validators(vital_ranges)
    results = validate_reading(validators, readings)
    out_of_range = [name for name, ok in results.items() if not ok]
    return {
        "valid": len(out_of_range) == 0,
        "results": results,
        "out_of_range": out_of_range,
    }
