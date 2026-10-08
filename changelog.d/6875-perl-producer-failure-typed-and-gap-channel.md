<!-- section: Fixed -->
- Perl: a configured `[perl].producer` exporter that fails (spawn error,
  timeout, non-zero exit) now reaches the typed record — a `failed`
  `language_runs[]` entry and a `producer_failure` limitation with
  `inspect_failure` recovery carrying the exporter error verbatim — instead
  of the generic "requires a fact packet" advice the user already followed;
  the run stays fail-closed (#6828).
- Perl: the documented `missing_discriminator` change fact field now forms
  the canonical gap (RIPR-SPEC-0064), so a spec-following packet produces a
  referenceable `canonical_gap_id` + `normalized_discriminator` instead of
  dropping the supplied discriminator; the `discriminator:`
  `changed_text_digest` prefix remains accepted as a compatibility channel
  and both channels form the identical gap identity (#6829).
