<!-- section: Fixed -->
- Uncased match-arm variant reads (`Kind::A`) no longer treat a name as a variant when the owner function, or an item in its body, carries a proc-macro attribute or a custom derive that may declare a local `Kind`; a compiler builtin on the owner, or the same attribute on a sibling function, still reads (RIPR-SPEC-0229, #7692).
