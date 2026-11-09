<!-- section: Changed -->

- The bare-name shadow check behind seam evidence no longer rescans each naming test's body once per (seam, test, oracle) ask: the item-syntax prefilter and its readability parse run once per distinct body and owner name per run, and the per-thread clean-parse memo hashes bodies with a word-at-a-time hasher instead of SipHash. On a diff that touches a large file this removes most of the repeated body scans. Output is unchanged (#13577).
