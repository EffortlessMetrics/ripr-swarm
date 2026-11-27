<!-- section: Internal / CI -->
- Windows all-targets compilation compiles again: the release observer scopes its unix directory mode to a rebound builder so the `unused_mut` warning no longer fails `-D warnings` on Windows; unix `0o700` behavior is unchanged (#16189).
