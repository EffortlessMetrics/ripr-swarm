<!-- section: Fixed -->
- Windows now has native controls for the verify-execute route (cancelling a command whose descendant holds the output pipes must return promptly) and for owned-process argument round-tripping, run in the advisory subset; a cross-platform test also pins that spawning a missing program returns a typed `NotFound` error (#9306, #9323).
- The process owner now has controls for termination at every stage of a child's life and for a failed termination surfacing as an incomplete-cleanup error, using a test-only failure-injection hook (#9323).
