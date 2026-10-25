<!-- section: Fixed -->
- TypeScript: a change to the pass-through return of a method decorator
  (`return original.apply(this, args) + 1`) is now credited by an exact
  assertion on the decorated method's result. Wrappers that return a cached
  or stored value, return conditionally, are async, or decorate anything but
  a lone bare `@D` on an instance method keep the `decorator_indirection`
  reading (#10189).
