<!-- section: Changed -->
- `assert!(hit)` / `assert!(!hit)` on an immutable once-bound `let` of a
  `-> bool` owner call now pins the owner's result like
  `assert_eq!(hit, true)`, so the let-bound spelling no longer reads as a
  false weak oracle (RIPR-SPEC-0197 bool-owner pins, #6718, #9074).
