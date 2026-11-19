<!-- section: Fixed -->
- Predicate `assert_eq!` oracles in a test file composed through a gated
  `mod` declaration are admitted only under the shared-gate rule: the owner
  must be reached solely through that same declaration, so an unknown feature
  gate no longer grants credit to an owner the crate also compiles without it
  (RIPR-SPEC-0197, #14537, #14599). The rule reaches a test declared in an
  ungated child module of the gated `mod` (the retry skips the refused
  declaration's own edge), and it never turns a provably-off gate
  (`cfg(any())`, an always-off `cfg_attr`) into a limit of ripr's reading: a
  value guard inside such a module keeps the module-declaration refusal, so no
  gap is withheld for a test no build compiles (RIPR-SPEC-0240).
