"""Rates and limits, one function per test style."""


def sales_tax(subtotal):
    return subtotal * 8 // 100


def late_fee(days):
    return days * 5


def commission(sale):
    return sale * 3 // 100


def handling(weight):
    return weight * 12


def rounding_unit(value):
    return value // 10 * 10
