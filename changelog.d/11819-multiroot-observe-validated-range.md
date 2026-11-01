<!-- section: Fixed -->
- The VS Code multi-root acceptance test matches the code-action range the host actually sends to the provider (the diagnostic range clamped to its line) instead of the server's unclamped end, so the real-client trace observation is found (#11819).
