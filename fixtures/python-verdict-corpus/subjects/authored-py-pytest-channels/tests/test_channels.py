from unittest.mock import ANY, Mock

from channels.core import notify_d, notify_d2, notify_e, notify_f, notify_g, notify_h, save_c1, save_c2, say_a, say_i, say_j, warn_b


def test_say_a(capsys):
    out = capsys.readouterr().out
    say_a("x")
    assert out == ""


def test_warn_b(capsys):
    warn_b("x")
    assert capsys.readouterr().out == ""


def test_save_c1(tmp_path):
    p1 = tmp_path / "a"
    p2 = tmp_path / "b"
    save_c1(p1, p2)
    assert p1.read_text() == "one"


def test_save_c2(tmp_path):
    p1 = tmp_path / "a"
    p2 = tmp_path / "b"
    save_c2(p1, p2)
    assert p2.read_text() == "two"


def test_notify_d():
    m = Mock()
    notify_d(m)
    m.send.assert_called_with("b")


def test_notify_d2():
    m = Mock()
    notify_d2(m)
    m.send.assert_any_call("b")


def test_notify_e():
    m = Mock()
    notify_e(m)
    m.send.assert_called_once()


def test_notify_f():
    m = Mock()
    notify_f(m)
    m.send.assert_called_once_with(ANY)


def test_notify_g():
    m = Mock()
    notify_g(m, 1)
    m.send.assert_called_once_with(1)


def test_notify_h():
    m = Mock()
    notify_h(m, 1)
    m.send.assert_called_once_with(1)


def test_say_i(capfd):
    say_i("x")
    assert capfd.readouterr().out == "hi x\n"


def test_say_j(capsys):
    say_j("x")
    captured = capsys.readouterr()
    assert captured.out == "hi x\n"
