def scale_setup(x):
    return x * 2


def scale_classattr(x):
    return x * 2


def scale_module(x):
    return x * 2


def scale_local(x):
    return x * 2


def scale_forward(x):
    return x * 2


def scale_cancel(x):
    return x * 2


def scale_pytest(x):
    return x * 2


def scale_literal(x):
    return x * 2


def scale_reference(x):
    return x * 2


def relay_forward(x):
    return scale_forward(x)


def reference_double(x):
    return x + x


def make_input():
    return 3


def scale_input(x):
    return x * 2
