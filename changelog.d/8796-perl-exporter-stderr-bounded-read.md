<!-- section: Fixed -->
- A failing managed Perl exporter's stderr capture is now read through a
  bounded trailing window instead of being loaded whole into memory, so an
  exporter that writes gigabytes to stderr can no longer exhaust ripr's
  memory while it builds the 600-character failure tail; the producer-failure
  outcome is unchanged (#8796).
  The tail is now exactly 600 characters (it was 601 through an
  off-by-one), and a blank trailing window steps back over earlier windows
  within a fixed 1 MiB scan budget (accumulating earlier windows so a
  diagnostic straddling a window boundary keeps its full tail) before
  reporting no tail.
