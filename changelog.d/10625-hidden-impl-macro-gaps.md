<!-- section: Fixed -->
- Operator and method calls by type path no longer earn sole-method credit when a statement macro in a function body that names the type, an argument-free item-level macro defined outside the workspace, or a blanket impl of a workspace trait with a default body for the method may define the impl (#10625).
