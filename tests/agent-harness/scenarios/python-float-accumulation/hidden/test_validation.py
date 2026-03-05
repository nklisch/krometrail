"""Hidden oracle tests — copied into workspace after agent finishes."""
import pytest
from bill import split_bill


def test_total_shares_equals_total_with_tip_three_people():
    # $47.00 * 1.18 = $55.46; buggy code returns total_shares=$55.47
    result = split_bill(47.00, 3)
    assert result["total_shares"] == result["total_with_tip"], (
        f"total_shares {result['total_shares']} != total_with_tip {result['total_with_tip']}"
    )


def test_total_shares_equals_total_with_tip_four_people():
    result = split_bill(61.00, 4)
    assert result["total_shares"] == result["total_with_tip"], (
        f"total_shares {result['total_shares']} != total_with_tip {result['total_with_tip']}"
    )


def test_total_shares_equals_total_with_tip_seven_people():
    result = split_bill(100.00, 7)
    assert result["total_shares"] == result["total_with_tip"], (
        f"total_shares {result['total_shares']} != total_with_tip {result['total_with_tip']}"
    )


def test_total_with_tip_correct():
    result = split_bill(100.00, 2, tip_pct=0.20)
    assert result["total_with_tip"] == pytest.approx(120.00, abs=0.01)


def test_per_person_correct():
    result = split_bill(60.00, 3, tip_pct=0.0)
    assert result["per_person"] == pytest.approx(20.00, abs=0.01)


def test_zero_tip_exact():
    result = split_bill(90.00, 3, tip_pct=0.0)
    assert result["total_with_tip"] == 90.00
    assert result["total_shares"] == 90.00


def test_shares_length_matches_num_people():
    result = split_bill(100.00, 5)
    assert len(result["shares"]) == 5


def test_total_shares_exact_match_various():
    for total, n, tip in [
        (50.00, 2, 0.18), (75.00, 4, 0.15), (33.33, 3, 0.20),
        (47.00, 3, 0.18), (100.00, 7, 0.18), (53.00, 6, 0.18),
    ]:
        result = split_bill(total, n, tip)
        assert result["total_shares"] == result["total_with_tip"], (
            f"total={total}, n={n}: total_shares={result['total_shares']} "
            f"!= total_with_tip={result['total_with_tip']}"
        )
