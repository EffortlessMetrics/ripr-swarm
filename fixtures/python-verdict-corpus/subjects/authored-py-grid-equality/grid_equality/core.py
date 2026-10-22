"""Equality tests, one function per test style."""


def is_admin(role):
    return role == "admin"


def is_saturday(day):
    return day % 7 == 6


def same_pair(a, b):
    return a == b


def is_https(port):
    return port == 443


def is_default_port(port):
    return port == 8080
