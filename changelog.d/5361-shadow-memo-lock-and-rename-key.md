<!-- section: Changed -->
- Cold `ripr pilot` evidence workers no longer hold a per-file lock while computing a type-shadow answer, mask each file once instead of once per probe key, and key the file-rename memo on a bounded byte sample instead of hashing the whole file for every lookup. Findings and artifacts are unchanged (#5361).
