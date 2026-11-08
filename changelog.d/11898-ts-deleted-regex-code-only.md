<!-- section: Fixed -->
- TypeScript deleted-line reading now skips regex-literal contents, so a removed `/\(/` no longer inflates paren depth and joins a following removed header into a non-header group; division and string `/` are unchanged (#11898).
