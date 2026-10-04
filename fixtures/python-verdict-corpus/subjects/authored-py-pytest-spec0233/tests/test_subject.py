import unittest
from types import SimpleNamespace

import pytest

from src.geometry import shift
from src.subject import (
    CartBound,
    CartOrthogonal,
    Settings,
    build_field,
    build_list,
    build_split,
    build_whole,
    fmt,
    label_call,
    label_getattr,
    label_lambda_text,
    parse_almost_equal,
    parse_and_chain,
    parse_approx,
    parse_assert_not_equal,
    parse_exact,
    parse_exact_first,
    parse_exact_second,
    parse_fluent,
    parse_isinstance,
    parse_not_equal,
    parse_self_compare,
    parse_with_patch_call,
    perr_broad,
    perr_exc_value,
    perr_match,
    perr_match_and_value,
    perr_match_anything,
    perr_raises_regex,
    perr_split_tests,
    perr_value_only,
)
from tests.fluent import assert_that

other = 3
expected = {"port": 1}


class FakeClient:
    def patch(self, path):
        return path


def test_exact():
    assert parse_exact("1") == 2


def test_not_equal():
    assert parse_not_equal("1") != 0


class TestNotEqual(unittest.TestCase):
    def test_assert_not_equal(self):
        self.assertNotEqual(parse_assert_not_equal("1"), 0)

    def test_almost_equal(self):
        self.assertAlmostEqual(parse_almost_equal("1"), 2.0)

    def test_raises_regex(self):
        self.assertRaisesRegex(KeyError, "empty", perr_raises_regex, "")


def test_isinstance():
    assert isinstance(parse_isinstance("1"), int)


def test_and_chain():
    assert parse_and_chain("1") == 2 and other == 3


def test_self_compare():
    assert parse_self_compare("1") == parse_self_compare("1")


def test_approx():
    assert parse_approx("1") == pytest.approx(2.0)


def test_fluent():
    assert_that(parse_fluent("1")).is_equal_to(2)


def test_exact_then_other():
    assert parse_exact_first("1") == 2
    assert other == 3


def test_other_then_exact():
    assert other == 3
    assert parse_exact_second("1") == 2


def test_raises_with_match():
    with pytest.raises(KeyError, match="empty"):
        perr_match("")


def test_raises_broad():
    with pytest.raises(KeyError):
        perr_broad("")


def test_value_only():
    assert perr_value_only("1") == 1


def test_raises_with_match_and_value():
    with pytest.raises(KeyError, match="empty"):
        perr_match_and_value("")
    assert perr_match_and_value("1") == 1


def test_a_split_raises():
    with pytest.raises(KeyError, match="empty"):
        perr_split_tests("")


def test_b_split_value():
    assert perr_split_tests("1") == 1


def test_raises_match_anything():
    with pytest.raises(KeyError, match=".*"):
        perr_match_anything("")


def test_raises_as_exc_value():
    with pytest.raises(KeyError) as exc:
        perr_exc_value("")
    assert str(exc.value) == "'empty'"


def test_dict_field():
    assert build_field()["timeout"] == 8080


def test_dict_whole():
    assert build_whole() == {"port": 80, "timeout": 8080}


def test_list_index():
    assert build_list()[1] == 8080


def test_a_split_dict_field():
    assert build_split()["timeout"] == 8080


def test_b_split_dict_whole():
    build_split()
    assert expected == {"port": 1}


def test_cart_orthogonal():
    unrelated = SimpleNamespace(total_orthogonal=lambda: 6)
    assert unrelated.total_orthogonal() == CartOrthogonal.LIMIT


def test_cart_bound():
    c = CartBound()
    assert c.total_bound() == 6


def test_label_call():
    assert label_call(1) == "b"


def test_label_lambda_text():
    assert label_lambda_text(1) == "lambda x"


def test_label_getattr():
    assert label_getattr(SimpleNamespace(a=1, b=2)) == 1


def test_patch_call_and_exact():
    client = FakeClient()
    client.patch("/x")
    assert parse_with_patch_call("1") == 2


def test_property():
    assert Settings().version == 1


def test_a_fmt_length():
    assert len(fmt(7)) == 4


def test_b_fmt_called():
    fmt(7)
    assert other == 3


def test_shift():
    assert shift(SimpleNamespace(x=2)) == 3
