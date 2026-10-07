<!-- section: Added -->
- Verdict corpus: 15 runtime-labeled cases in a new `authored-spec-confirm`
  subject cover spec cells that had no runtime-truth case. They include
  constant-token and mixed oracle confirmation (RIPR-SPEC-0094); `log::`,
  `eprintln!`, `drop(..)` and local-push sinks (0096); wrapper, shadowed and
  same-name constructor owners (0005); and trait-default and `?`-exit identity
  traps (0197). The five wrong verdicts are tracked in #6989, #6990, #6991 and
  #4478 (#6992).
