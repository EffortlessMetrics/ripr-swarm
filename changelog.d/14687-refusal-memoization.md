<!-- section: Changed -->
- Assertion refusal analysis now traces its split phase and reuses per-(test, owner) refusal, cfg-gate, and module-declaration computations instead of repeating tree walks for each assertion or probe (#14687).
