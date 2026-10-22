"""Owners for false-credit traps: every function has one test file that looks like it checks it."""


def dead_branch(a, b):
    return a * b


def live_branch(a, b):
    return a * b


def skipped_test(a, b):
    return a * b


def skipif_false(a, b):
    return a * b


def xfail_test(a, b):
    return a * b


def module_skipped(a, b):
    return a * b


def module_loaded(a, b):
    return a * b


def platform_guarded(a, b):
    return a * b


def platform_matched(a, b):
    return a * b


def misnamed_test(a, b):
    return a * b


def named_test(a, b):
    return a * b


def unused_helper(a, b):
    return a * b


def input_asserted(a, b):
    return a * b


def result_asserted(a, b):
    return a * b


def tuple_asserted(a, b):
    return a * b


def message_asserted(a, b):
    return a * b


def swallowed_assert(a, b):
    return a * b


def reraised_assert(a, b):
    return a * b


def tautology(a, b):
    return a * b


def truthy_only(a, b):
    return a * b


def patched_owner(a, b):
    return a * b


def wrapped_swallow(a, b):
    return a * b


def wrapped_through(a, b):
    return a * b


def mocked_send(order_id, sender):
    sender.send(order_id * 2)


def mocked_send_args(order_id, sender):
    sender.send(order_id * 2)


def sibling_arm(kind, qty):
    if kind == "single":
        return qty * 2
    return qty * 3


def edited_arm(kind, qty):
    if kind == "single":
        return qty * 2
    return qty * 3


def star_imported(a, b):
    return a * b


def empty_table(a, b):
    return a * b


def filled_table(a, b):
    return a * b
