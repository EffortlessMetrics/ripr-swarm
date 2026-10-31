<!-- section: Fixed -->
- `ripr pilot` no longer reruns the workspace macro scan for every free-helper hop: on tokio the evidence stage drops from about 318 s to 22 s (pilot 275 s timeout to 31 s) (#11055).
