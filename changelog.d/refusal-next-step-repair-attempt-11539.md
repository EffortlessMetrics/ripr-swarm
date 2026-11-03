<!-- section: Fixed -->
- `ripr agent receipt`, `ripr agent verify-execute` and the repair-attempt
  store refusals now say what to do next: start a new attempt with `ripr agent
  repair`, pass the matching `--store`, fix the `--result-json` path, or
  regenerate the packet, instead of only describing the failure (#11539).
