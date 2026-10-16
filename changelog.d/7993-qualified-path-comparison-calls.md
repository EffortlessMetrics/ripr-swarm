<!-- section: Fixed -->
- A comparison spelled as a type-path or qualified-self call
  (`Result::eq(&r, &x)`, `<_ as PartialEq>::eq(&r, &x)`) and a negated
  literal spelling (`!&true`) now read like their operator and literal, so
  an inequality asserted through them no longer earns `exact_value`/strong
  credit (RIPR-SPEC-0231 rule 1; #7993).
