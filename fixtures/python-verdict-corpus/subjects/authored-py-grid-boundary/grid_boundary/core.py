"""Relational boundaries, one function per test style."""


def is_adult(age):
    return age >= 18


def free_shipping(cents):
    return cents >= 5000


def can_retire(age):
    return age >= 67


def passes(score):
    return score >= 50


def overweight(grams):
    return grams >= 2000
