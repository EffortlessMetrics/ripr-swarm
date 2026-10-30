<!-- section: Fixed -->
- Git failures no longer dump debug-format argv or unbounded output: the
  shared `run_git` failure message and the spawn/timeout describe string now
  render the command line as typed and bound both streams at 500 escaped
  characters, matching the documented contract. An unclassified `git diff`
  failure (one no hint branch recognizes) now names the manual probe
  (`git -C <root> diff <range>`) instead of leaving raw stderr as the only
  text (#9604).
