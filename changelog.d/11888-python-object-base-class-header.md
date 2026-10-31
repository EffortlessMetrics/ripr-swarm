<!-- section: Fixed -->
- Python: dropping or adding a sole `object` base (`class X(object):` to `class X:`) no longer reads `exposed` from any test that reaches the class. Python 3 treats the two headers as the same class, so the edit earns no probe; any other header edit (a different base, `object` among several bases, a changed `metaclass=`) keeps its verdict (#11888).
