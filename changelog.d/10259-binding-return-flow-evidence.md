<!-- section: Added -->
- Rust: a changed `let` whose binding reaches the function's returned expression unchanged (`let end = sku.find('-').unwrap_or_else(|| sku.len()); &sku[..end]`) now names that flow in the finding evidence (`binding_return_relation`) instead of only the generic static-unknown reason. The finding keeps its class and gains no credit (RIPR-SPEC-0357 stage 1, #10259).
