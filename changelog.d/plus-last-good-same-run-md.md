<!-- section: Fixed -->
- `ripr plus`: a last-good Markdown file is kept only when it is the
  projection of the last-good JSON this run actually saved. A leftover `.md`
  from another run is dropped rather than saved beside a newer `.json`, and
  Markdown is not copied when the JSON copy fails
  ([#6698](https://github.com/EffortlessMetrics/ripr-swarm/issues/6698)).
