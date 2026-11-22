from fx.core import is_musl


def test_musl():
    assert is_musl("musllinux_1_1_x86_64") is True


def test_short_prefix():
    assert is_musl("musx") is True
