<!-- section: Added -->
- TypeScript verdict corpus: `fixtures/typescript-verdict-corpus` holds 101
  authored cases across jest, vitest, mocha with chai and node:test,
  including at least one case for RIPR-SPEC-0243 acceptance examples 1 to
  32, 34 and 35 (example 33 is #7099). Each case is labeled by running
  its mutants against the package's own tests: StrykerJS mutants for most
  rewrites, hand-applied mutants for two handed-over subjects, and the
  edit itself for a behavior change. `cargo xtask verdict-corpus check --language typescript` scores
  them: today 27/62 false actionable, 9/39 false exposed, 0/39 false
  silent. The DX scoreboard gates those three rates against the committed
  baseline (#6686).
