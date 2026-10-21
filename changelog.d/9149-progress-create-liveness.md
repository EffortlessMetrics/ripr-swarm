<!-- section: Fixed -->
- LSP analysis refresh and shutdown proceed when a progress-capable client
  leaves progress creation unanswered; late responses cannot revive ended
  progress tokens. A suspended notification on one progress token no longer
  blocks another token's publishing report or completion (#9149).
