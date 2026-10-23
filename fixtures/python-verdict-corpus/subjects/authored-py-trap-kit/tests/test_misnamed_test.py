from trapkit.core import misnamed_test


def check_misnamed_total():
    assert misnamed_test(3, 7) == 21
