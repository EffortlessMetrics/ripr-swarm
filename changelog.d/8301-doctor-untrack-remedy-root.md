<!-- section: Fixed -->
- The `ripr doctor` tracked-artifact warning now names the selected `--root` in its untrack command (`git -C <root> rm --cached -r -- target/ripr`), so pasting it from another directory cannot untrack files in the wrong repository. The current-directory root keeps the short form (#8276).
