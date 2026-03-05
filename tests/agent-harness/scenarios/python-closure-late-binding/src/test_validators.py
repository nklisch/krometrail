"""Visible failing test — agent can see and run this."""
from validators import make_validators


def test_each_validator_has_own_range():
    validators = make_validators([
        ("small", 0.0, 10.0),
        ("medium", 50.0, 60.0),
        ("large", 100.0, 200.0),
    ])

    _name_s, check_small = validators[0]
    _name_m, check_medium = validators[1]
    _name_l, check_large = validators[2]

    # 5.0 is within "small" range [0, 10] — should pass
    assert check_small(5.0) is True, (
        "5.0 should be valid for 'small' range [0, 10]"
    )

    # 5.0 is NOT within "medium" range [50, 60] — should fail
    assert check_medium(5.0) is False, (
        "5.0 should be invalid for 'medium' range [50, 60]"
    )


def test_validators_do_not_all_use_last_range():
    validators = make_validators([
        ("narrow", 0.0, 1.0),
        ("wide", 0.0, 1000.0),
    ])

    _, check_narrow = validators[0]
    _, check_wide = validators[1]

    # 500 should fail the narrow range but pass the wide range
    assert check_narrow(500.0) is False, (
        "500.0 should be invalid for 'narrow' range [0, 1]"
    )
    assert check_wide(500.0) is True, (
        "500.0 should be valid for 'wide' range [0, 1000]"
    )
