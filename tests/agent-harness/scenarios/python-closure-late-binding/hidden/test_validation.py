"""Hidden oracle tests — copied into workspace after agent finishes."""
from validators import check_patient_vitals, make_validators, validate_reading


def test_each_validator_independent():
    validators = make_validators([
        ("small", 0.0, 10.0),
        ("medium", 50.0, 60.0),
        ("large", 100.0, 200.0),
    ])
    _, check_small = validators[0]
    _, check_medium = validators[1]
    _, check_large = validators[2]

    assert check_small(5.0) is True
    assert check_small(15.0) is False
    assert check_medium(55.0) is True
    assert check_medium(5.0) is False
    assert check_large(150.0) is True
    assert check_large(50.0) is False


def test_narrow_vs_wide():
    validators = make_validators([
        ("narrow", 0.0, 1.0),
        ("wide", 0.0, 1000.0),
    ])
    _, check_narrow = validators[0]
    _, check_wide = validators[1]

    assert check_narrow(0.5) is True
    assert check_narrow(500.0) is False
    assert check_wide(500.0) is True


def test_boundary_values():
    validators = make_validators([("range", 10.0, 20.0)])
    _, check = validators[0]

    assert check(10.0) is True, "Low boundary should be inclusive"
    assert check(20.0) is True, "High boundary should be inclusive"
    assert check(9.99) is False
    assert check(20.01) is False


def test_validate_reading():
    validators = make_validators([
        ("temp", 36.0, 38.0),
        ("pulse", 60.0, 100.0),
    ])
    results = validate_reading(validators, {"temp": 37.0, "pulse": 55.0})
    assert results["temp"] is True
    assert results["pulse"] is False


def test_patient_vitals_normal():
    readings = {
        "temperature": 36.6,
        "heart_rate": 72.0,
        "blood_pressure_sys": 120.0,
        "blood_pressure_dia": 80.0,
        "oxygen_saturation": 98.0,
    }
    result = check_patient_vitals(readings)
    assert result["valid"] is True, f"Normal vitals flagged: {result['out_of_range']}"


def test_patient_vitals_fever():
    readings = {
        "temperature": 39.5,
        "heart_rate": 72.0,
        "blood_pressure_sys": 120.0,
        "blood_pressure_dia": 80.0,
        "oxygen_saturation": 98.0,
    }
    result = check_patient_vitals(readings)
    assert "temperature" in result["out_of_range"], (
        f"Fever should be flagged. Results: {result['results']}"
    )


def test_patient_vitals_low_oxygen():
    readings = {
        "temperature": 36.6,
        "heart_rate": 72.0,
        "blood_pressure_sys": 120.0,
        "blood_pressure_dia": 80.0,
        "oxygen_saturation": 88.0,
    }
    result = check_patient_vitals(readings)
    assert "oxygen_saturation" in result["out_of_range"]


def test_regression_validators_use_own_bounds():
    """Confirm that closures capture their own loop iteration's values."""
    validators = make_validators([
        ("a", 0.0, 5.0),
        ("b", 10.0, 15.0),
        ("c", 20.0, 25.0),
    ])
    # Value 3.0 should only pass validator "a"
    for name, check in validators:
        if name == "a":
            assert check(3.0) is True, f"3.0 should pass validator 'a'"
        else:
            assert check(3.0) is False, f"3.0 should fail validator '{name}'"
