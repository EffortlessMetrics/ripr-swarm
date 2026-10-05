<!-- section: Added -->
- Verdict corpus: 14 more runtime-labeled grid cases in four authored crates
  (`authored-grid-flow`, `-bindings`, `-results`, `-bits`) for spec cells the
  operator grid left open: a lone `} else {` line and field-assignment
  activation (RIPR-SPEC-0001), an Err-return test guard (RIPR-SPEC-0154),
  shadowed and reassigned bindings (RIPR-SPEC-0157), a helper reached with
  computed arguments (RIPR-SPEC-0159), routed and swallowed Result matches
  (RIPR-SPEC-0175), a boundary on a counter of input bytes, and bitwise `|`
  and `<<`. ripr reads 3 ideal, 5 abstained and 6 false actionable, with no
  false exposed or false silent verdict.
