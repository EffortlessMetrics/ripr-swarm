from lits.cfg import (
    labels,
    limits,
    ports,
    roles,
    server_config,
    staff,
    timeouts,
    whole_config,
)


def test_server_retries_are_three():
    assert server_config()["retries"] == 3


def test_ports_repr_names_the_admin_port():
    assert "8080" in repr(ports())


def test_low_limit_is_ten():
    assert limits()["low"] == 10


def test_whole_config_is_pinned():
    assert whole_config() == {"mode": "fast", "level": 2}


def test_first_role_is_admin():
    assert roles()[0] == "admin"


def test_second_staff_member_is_editor():
    assert staff()[1] == "editor"


def test_read_timeout_is_thirty():
    assert timeouts().get("read") == 30


def test_label_name_is_x():
    assert labels()["name"] == "x"
