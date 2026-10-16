<!-- section: Internal / CI -->
- Fixture-corpus CLI children use one Rayon worker while the parent
  parallelizes cases, bounding nested worker pools (#8226).
