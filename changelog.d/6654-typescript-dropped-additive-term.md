<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0248): on a return-value, field or predicate
  seam, a change that only adds or drops one additive term
  (`total + shippingFee(subtotal)` to `total`) no longer reads `exposed`
  unless a related test makes a strong value assertion (`exact_value`,
  `whole_object_equality` or `snapshot`) on the whole result of an owner
  call whose input lets ripr fold the term to a non-zero number.
  Otherwise it reads `weakly_exposed` and asks for such an input, since
  an exact assertion passes on both versions wherever the term is 0. On
  the TypeScript verdict corpus false verdicts drop from 10/101 to 9/101
  and false exposed from 1/39 to 0/39 (#6654).
