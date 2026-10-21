<!-- section: Fixed -->
- Python return-value changes that follow an earlier `if X: return ...` in the
  same block no longer read `exposed` when every strong related test call
  satisfies `X` and leaves the owner before the changed line. The finding
  reports `weakly_exposed` naming the missing discriminator `not (X)`; unreadable
  or non-terminating guards keep the existing verdict (#9002).
