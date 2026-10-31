<!-- section: Internal / CI -->
- `check-user-text` no longer counts the digest's own stage vocabulary (the `discriminator` token in the `Evidence:` line and the three legend phrases that define it), so a new golden that carries the digest no longer reads as +3 banned lines. Other uses of the word, such as "No strong discriminator", are still counted (#11385).
