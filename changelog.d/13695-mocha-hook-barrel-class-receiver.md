<!-- section: Fixed -->
- A mocha setup hook that builds a class imported through a barrel no longer
  turns that class's method findings into `static_unknown`; the setup-hook
  guard reads only plain owner calls, as it already did for a direct import
  of the class (#13592).
