<!-- section: Fixed -->
- TypeScript: a `toThrow` test that calls the function with the exact input of
  a changed throw guard (`if (n < 0) throw ...` with `f(0)`) now closes the
  boundary gap; a throw test at another input still does not (#13601).
