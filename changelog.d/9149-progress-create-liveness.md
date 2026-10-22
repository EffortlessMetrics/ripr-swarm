<!-- section: Fixed -->
- LSP analysis refresh and shutdown proceed when a progress-capable client
  leaves progress creation unanswered; late responses cannot revive ended
  progress tokens. A suspended notification on one progress token no longer
  blocks another token's publishing report or completion. Cancellation while
  waiting for terminal cleanup preserves retry custody for every drain token
  (#9149).
