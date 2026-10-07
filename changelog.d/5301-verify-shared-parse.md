<!-- section: Changed -->
- Performance (`ripr agent verify`): each snapshot is parsed twice per verify
  instead of four times. Validation no longer re-parses the document
  as an untyped value just to re-prove well-formedness, and the
  outcome report reuses the validated repository heads instead of
  parsing each document again to read them. Verify wall time on a
  119MB snapshot pair fell from 11.0s to 8.8s (medians of three
  runs), with byte-identical verify output (#5301).
