<!-- section: Fixed -->
- Python: rewriting what a method records into a private container
  (`self._seen.append(n + 1)`) no longer reads `exposed` through a test that
  only pins the method's own result. It reads `weakly_exposed` and names the
  functions that read the container, unless a related test names the
  container or calls one of them (#16010).
