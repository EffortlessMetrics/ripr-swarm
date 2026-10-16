<!-- section: Internal / CI -->
- Fixture/golden CLI children use one Rayon worker while the parent
  parallelizes cases, bounding nested worker pools (#8226).
