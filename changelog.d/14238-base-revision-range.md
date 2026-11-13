<!-- section: Changed -->
- A revision range passed to `--base` or `--head` (`--base main..HEAD`) is now answered as a range: the refusal says the flag takes one revision and names the value to pass (`--base main`), instead of asking for a spelling check or a fetch of a branch named `main..HEAD` (#14238).
