<!-- section: Fixed -->
- The TypeScript preview adapter reads a changed counted-loop header
  (`for (let i = 0; i < attempts; i++)`) that a test observes only through a
  mock call count as `static_unknown` instead of a `weakly_exposed` gap: it does
  not model how many iterations the loop runs, so it no longer asks for a
  repair the test may already satisfy (#9496).
