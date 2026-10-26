<!-- section: Fixed -->
- The evidence-promotion honesty corpus now pins two macro-table false promotions as `no_static_path`: a fused `::` token read as two `:` puncts, and an arrow table whose body declares its own scope (#10298).
- The same corpus pins an arrow-table body that binds a local callable named like the owner (`let gate = ..`) as `no_static_path` (#10298).
- The same corpus also pins an arrow-table test that binds the callee in a `for` pattern and one that invokes a macro defined in another module, both as `no_static_path` (#10298).
- The same corpus pins an arrow-table test with an exported assertion-named macro invoked beside it as `no_static_path` (#10298).
- The same corpus pins a nested block comment before a foreign macro bang and a non-ASCII macro name ending in an allowed name, both as `no_static_path` (#10298).
