<!-- section: Fixed -->
- `ripr explain` no longer prints the producer phrase `no oracle row` for a
  related test whose assertion text ripr did not retain: the row now reads
  `assertion not recorded` — an honest retention limit instead of record-shape
  vocabulary that contradicts an assertion the user finds in the source — and
  the bare fallback is suppressed when another row of the same test already
  printed its assertion (#8746).
