"""Comprehensions, generator expressions and generators."""


def squares_of_evens(nums):
    return [n * n for n in nums if n % 2 == 0]


def index_by_name(rows):
    return {row["name"]: row["id"] for row in rows}


def total_lengths(words):
    return sum(len(w) for w in words)


def distinct_pairs(a, b):
    return [(x, y) for x in a for y in b if x != y]


def countdown(start):
    while start > 0:
        yield start
        start -= 1
