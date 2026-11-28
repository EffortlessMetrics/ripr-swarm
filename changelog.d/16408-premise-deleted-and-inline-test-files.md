<!-- section: Fixed -->
- The commit step `ripr check`, `ripr pilot` and the editor print before a
  repair start now also names deleted files and non-target inline-test
  files the start refuses over. A deleted
  tracked file, and a dirty file carrying an inline `#[cfg(test)]` module that
  is not the repair's own target, were left out, so the start still refused
  after the printed commit (or no commit step was shown at all). Running the
  printed commit now clears the start's "commit first" refusal in those
  cases. Symlinks, oversized or unreadable files and out-of-surface test
  files are still not named. The refusal itself is unchanged (#16408).
