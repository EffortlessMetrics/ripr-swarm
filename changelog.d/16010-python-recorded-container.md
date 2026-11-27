<!-- section: Fixed -->
- Python: rewriting what a method records into a private container
  (`self._seen.append(n + 1)`) no longer reads `exposed` through a test that
  only pins the method's own result. It reads `weakly_exposed`, names the
  functions that read the container and asks for an assertion on the
  recorded values. A related test that names the container or calls one of
  those functions keeps the existing verdict (#16010).
