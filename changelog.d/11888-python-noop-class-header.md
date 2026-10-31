<!-- section: Fixed -->
- A Python 3 no-op class-header edit (`class X(object):` to `class X:`, either direction) no longer
  reads `exposed`; the probe is withheld, while any other base-list or metaclass change keeps its
  probe (#11888).
