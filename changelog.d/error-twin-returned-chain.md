<!-- section: Fixed -->
- `return Err(X).context(..)` and other returns of a method chain on an error
  constructor now give one `error_variant` seam, on `Err(X)`, instead of a
  second one on the whole `return`. A `return x.map_err(..)` with no
  constructor inside keeps its own seam. Classified seam caches move to
  1.44 / 0.50, so warm entries rebuild (#6935).
