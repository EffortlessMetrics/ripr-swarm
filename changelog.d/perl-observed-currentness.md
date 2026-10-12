<!-- section: Fixed -->
- Perl: a fact-packet finding whose source is on disk (digest verified) and
  whose change range contains a line the diff adds, with matching text in
  the source, is now
  `candidate_current`. It now appears in the default human report, SARIF
  results, GitHub annotations, `finding_alignment`
  items and the diff badge's exposure-gap count, as candidate-current Python
  and TypeScript preview findings already do. Fixture-only packets and changes
  the diff does not touch stay `unresolved_subject`. A packet whose on-disk
  source cannot be read is now rejected instead of skipping its digest check
  (#6586).
