import unittest

from derived.core import scale_pytest


def test_scale():
    expected = scale_pytest(2)
    assert scale_pytest(2) == expected
