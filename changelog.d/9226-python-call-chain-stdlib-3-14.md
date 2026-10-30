<!-- section: Fixed -->
- Python: the call-chain credit also refuses an owner imported under a Python 3.14 standard-library name (`annotationlib`, `compression`, and others), a frozen `__hello__` module, or a module removed since 3.9. On 3.14 the interpreter already holds the real `annotationlib`, so a workspace `annotationlib.py` never runs (#9226).
