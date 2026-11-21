<!-- section: Internal / CI -->
- The #1609 qualified release producer now packages `ripr` from the same
  publish stage as the real publish, together with `ripr-contract`, and accepts
  only entries that are committed blobs or the exact inline-test strip of one,
  with the stage marker naming the admitted commit (#15204).
