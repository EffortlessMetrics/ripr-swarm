<!-- section: Fixed -->
- The goldens gate now compares the full wording of every fixed-text `no repair start offered` refusal arm instead of masking everything after its opening, so rewording one shows as golden drift. Only the inventory and classification-failure arms, which embed a language, file or command, still mask their trailing value (#12003).
