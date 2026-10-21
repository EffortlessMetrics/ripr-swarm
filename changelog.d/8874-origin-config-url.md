<!-- section: Fixed -->
- Back-sync and source-promotion origin gates judge the checkout's configured
  `remote.origin.url` instead of Git's `insteadOf`-rewritten URL, so a foreign
  origin cannot be rewritten into an approved one (#8874).
