<!-- section: Internal / CI -->
- Bind advisory preflight reuse to the executed source tree, base, command plan
  and observed local context; retain original execution identity when carrying
  results across source-equivalent commits (#8024).
- Refuse advisory receipt reuse when command exit evidence contradicts its
  status or the producer has not explicitly declared reuse available (#8024).
