<!-- section: Fixed -->
- A pinned assertion that passes only an empty literal for a slice or array
  parameter the changed tail iterates (`assert_eq!(total_empty([]), 0)` over
  `values.iter().sum::<u32>()`) no longer credits that tail, so the gap stays
  visible instead of reading as covered (#9444, RIPR-SPEC-0265 rule 2).
