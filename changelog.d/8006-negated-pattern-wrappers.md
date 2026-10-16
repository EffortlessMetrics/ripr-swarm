<!-- section: Fixed -->
- A negation of a whole assertion condition that holds a `matches!`
  (`!(flag || matches!(..))`, `!{ matches!(..) }`,
  `Some(matches!(..)).unwrap().not()`, `(matches!(..),).0.eq(&false)`), a
  block under `^` or `|`, and a comparison, pattern, `||`, `&&`,
  relational operator or comparison call compared with a non-literal side
  (`assert_eq!(r == Err(E::Bad), expected)`) no longer
  earn `exact_value`/strong credit (RIPR-SPEC-0231 rule 1; #8006).
