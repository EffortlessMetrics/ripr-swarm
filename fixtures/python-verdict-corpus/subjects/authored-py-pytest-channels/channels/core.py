import sys


def say_a(name):
    print("hello " + name)


def warn_b(name):
    print("bad " + name, file=sys.stderr)


def save_c1(a, b):
    a.write_text("one")
    b.write_text("two")


def save_c2(a, b):
    a.write_text("one")
    b.write_text("two")


def notify_d(sender):
    sender.send("a")
    sender.send("b")


def notify_d2(sender):
    sender.send("a")
    sender.send("b")


def notify_e(sender):
    sender.send("a")


def notify_f(sender):
    sender.send("a")


def notify_g(sender, n):
    sender.send(abs(n))


def notify_h(sender, n):
    sender.send(n)


def say_i(name):
    print("hi " + name)


def say_j(name):
    print("hi " + name)
