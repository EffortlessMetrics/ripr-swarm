import pytest

from guards.errors import check_a, check_c, check_e, check_f


def test_callable_type_only():
    pytest.raises(ValueError, check_a, "")


def test_callable_with_match_kwarg():
    pytest.raises(ValueError, check_c, "", match="empty")


def test_bound_args_index():
    with pytest.raises(ValueError) as exc:
        check_e("")
    assert exc.value.args[0] == "empty"


def test_bound_length_only():
    with pytest.raises(ValueError) as exc:
        check_f("")
    assert len(str(exc.value)) == 5
