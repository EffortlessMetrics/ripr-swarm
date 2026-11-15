<!-- section: Changed -->
- `ripr agent stub` builds a method's receiver from a constructor instead of
  a `todo!()` when the impl establishes one: the first all-literal call an
  existing test in the target module makes (`TokenBucket::new(5)`), else a
  no-argument constructor, else `Type::default()`. Human output names the
  source on a `Receiver:` line. On the 34-case stub corpus this removes the
  receiver fill-in from 4 of the 14 offered stubs (#8361).
