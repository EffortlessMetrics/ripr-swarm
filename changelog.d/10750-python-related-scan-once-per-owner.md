<!-- section: Changed -->
- Python diff mode finds an owner's related tests once per run instead of once per changed line, scans large test sets on all cores, and checks an import's module before scanning the test body for rebinding. A two-file pandas diff drops from about 290 s to about 15 s, and sympy from about 31 s to about 12 s, with byte-identical output (#10750).
