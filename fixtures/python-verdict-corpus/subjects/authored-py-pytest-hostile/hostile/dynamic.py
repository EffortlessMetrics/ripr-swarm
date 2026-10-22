def run(op, a, b):
    return globals()["_op_" + op](a, b)


def _op_add(a, b):
    return a + b


def _op_sub(a, b):
    return a - b
