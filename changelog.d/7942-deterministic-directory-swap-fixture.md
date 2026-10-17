<!-- section: Internal / CI -->
- Directory-swap cache tests retain their FULL and compact restart journeys without
  racing live ctime ticks; recorded stat tuples and raw metadata checks cover
  device and inode cache identity deterministically (#7942).
