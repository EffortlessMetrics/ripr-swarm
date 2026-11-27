<!-- section: Fixed -->
- `ripr check --diff -` that produces zero findings now says the diff piped
  on stdin may not be a valid unified diff, instead of referring to a diff
  file the caller never named (#11510).
- Python checks: `cls.` and `super().` spellings of the unittest
  assertions `assertAlmostEqual`, `assertRaises` and `assertRaisesRegex`
  are now read like their `self.` forms. Before, only the `self.` receiver
  was recognized for these assertions (#11523, #7567).
- TypeScript and JavaScript preview: a test that declares its own function
  or `const`/`let`/`var` with the owner's name on the same line as other
  code (after `{`, `}` or `;`) now shadows the owner, as the multi-line form
  already did, so its assertion no longer credits the changed owner as
  `exposed` (#11711, #11521).
- With an explicit `ripr agent repair --store`, the edit cage limits the
  attempt's own writes to that attempt's directory in the store,
  `ripr agent status` follow-up commands carry the resolved store, and
  after-phase refusals name the receipt path inside it. On Windows, a store
  spelling that differs from the store's on-disk name only in letter case
  is refused instead of sharing its identity (#12483, #4981).
- Rust related-test matching now types a method receiver only from a `let`
  binding still in scope at the call: a binding whose block has ended, or a
  name shadowed by a closure parameter or `for` pattern, no longer types the
  receiver, and a receiver bound to an associated item that is not an enum
  variant (such as `Site::DEFAULT_CACHE`) is not assumed to be of that type.
  Such a test is no longer related to or credited for another type's method
  (#12527, #6858).
