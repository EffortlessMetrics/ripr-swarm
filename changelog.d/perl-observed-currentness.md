<!-- section: Fixed -->
- Perl: a fact-packet finding whose source is on disk (digest verified) and
  whose change range contains a line the diff adds is now
  `candidate_current`, so the default human report shows it instead of
  hiding it as `unresolved_subject`. Fixture-only packets and changes the
  diff does not touch stay `unresolved_subject` (#6586).
