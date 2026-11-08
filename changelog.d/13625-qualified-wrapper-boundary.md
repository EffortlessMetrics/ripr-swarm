<!-- section: Fixed -->
- A changed boundary reached through a free wrapper now reads `exposed` whether
  the test calls the wrapper through a bare import or a module-qualified path
  such as `krate::inner::entry(10)`; a path naming another module's same-named
  function still earns no credit (#13625).
