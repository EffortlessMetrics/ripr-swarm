"""An owner module that imports os (RIPR-SPEC-0233 example 27)."""

import os


def shift(pos):
    return pos.x + 1


def workdir():
    return os.getcwd()
