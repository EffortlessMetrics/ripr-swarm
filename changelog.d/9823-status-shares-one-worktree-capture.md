<!-- section: Changed -->
- `ripr agent status` captures the worktree once per call for finished attempts that share a cage policy, instead of once per attempt; a later call always recaptures, so an edit between calls is never hidden. Two finished attempts over 3000 tracked files drop from about 1.3-1.6 s to about 0.8 s (debug build) (#9823).
