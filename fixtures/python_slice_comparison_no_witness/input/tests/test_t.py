from fx.core import is_musl

MUSL = "musllinux_1_1_x86_64"
MANY = "manylinux_2_17_x86_64"


def test_musl():
    assert is_musl(MUSL) is True


def test_many():
    assert is_musl(MANY) is False
