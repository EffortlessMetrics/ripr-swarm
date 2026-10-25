<!-- section: Changed -->
- A closed-form boundary operand no longer credits the observed input when the owner rebinds the parameter through a destructuring `let` (including `let-else`), a closure parameter pattern, a multi-line match-arm binding, or a same-line earlier `let`; the scan is token-level and fails closed (#9774).
