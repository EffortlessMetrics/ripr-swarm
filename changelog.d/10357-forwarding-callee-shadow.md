<!-- section: Fixed -->
- Refuse helper-chain result forwarding when a hop caller lexically rebinds
  the callee with a nested `fn`, `const`, `static`, or a `use` of that name
  (#10357 item 2). Unrelated `use` items in the same wrapper still forward.
  Boundary pairing already relied on tail forwarding; the rebinding scan now
  runs on that path too.
