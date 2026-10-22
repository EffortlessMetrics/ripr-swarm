<!-- section: Fixed -->
- The Windows shard workflow bounds each shard with `timeout`, so a run nextest cannot end is intended to stop at 44 minutes with its log and exit status uploaded; the SLOW rows nextest had printed by then usually name the stuck tests (#9272). A real hang has not yet exercised this path.
