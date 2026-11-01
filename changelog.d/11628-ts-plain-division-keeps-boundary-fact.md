<!-- section: Fixed -->
- A TypeScript changed line with a plain division after the comparison (`if (amount >= threshold) { return Math.floor(amount / 10); }`) no longer loses its boundary fact and repair packet. A `/` that follows a value (identifier, number, `)` or `]`) counts as division; `//`, `/*`, and any regex-position `/` still refuse the line (#11628).
