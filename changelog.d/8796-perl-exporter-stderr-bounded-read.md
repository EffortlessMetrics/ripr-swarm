<!-- section: Fixed -->
- A failing managed Perl exporter's stderr capture is now read through a
  bounded trailing window instead of being loaded whole into memory, so an
  exporter that writes gigabytes to stderr can no longer exhaust ripr's
  memory while it builds the 600-character failure tail; the producer-failure
  outcome is unchanged (#8796).
