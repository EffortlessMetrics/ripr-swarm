<!-- section: Fixed -->
- On macOS, a git-spelled NFC changed path now matches the NFD on-disk name from the workspace walk, so Instant no longer drops the file. Linux and Windows keep byte-distinct NFC/NFD names as different files. Display still uses git's spelling (#8419).
