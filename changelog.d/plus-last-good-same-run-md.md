<!-- section: Fixed -->
- `ripr plus`: a last-good Markdown file is kept only when it is the
  projection of the last-good JSON this run actually saved. Leftover `.md`
  is dropped only after that JSON save, so a failed JSON copy leaves a
  previous matching pair intact and a newer `.json` cannot sit beside
  another run's `.md`. Unreadable canonical Markdown is a Markdown-only
  failure: JSON is still saved. Skip-rewrite of matching last-good
  Markdown applies only to a regular file; a symlink or FIFO is dropped
  and replaced
  ([#6698](https://github.com/EffortlessMetrics/ripr-swarm/issues/6698)).
