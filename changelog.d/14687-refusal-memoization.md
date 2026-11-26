<!-- section: Changed -->
- Assertion refusal analysis now traces its split phase and reuses per-(test, owner) refusal, cfg-gate, and module-declaration computations within each probe instead of repeating tree walks for each assertion of that probe (#14687).
