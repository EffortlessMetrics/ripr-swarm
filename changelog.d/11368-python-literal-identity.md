<!-- section: Fixed -->
- Python finding identities now preserve literal whitespace and comment boundaries (affected ids carry a versioned `v1-`/`v1u-` marker) and file findings are withheld with a typed `producer_failure` limitation when source context cannot be established; suppressions keyed to the earlier ambiguous ids no longer match (#11368).
