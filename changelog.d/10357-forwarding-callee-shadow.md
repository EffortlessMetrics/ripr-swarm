<!-- section: Fixed -->
- Refuse helper-chain result forwarding when a hop caller lexically rebinds
  the callee with a nested `fn`, `const`, `static`, or `use` (#10357 item 2).
  Boundary pairing already relied on tail forwarding; the identity scan now
  runs on that path too.
