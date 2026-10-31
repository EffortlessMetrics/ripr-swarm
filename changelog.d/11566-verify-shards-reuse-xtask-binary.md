<!-- section: Internal / CI -->
- The hosted required run's shard verify job no longer installs a toolchain and recompiles xtask after the last shard finishes; shard 1 hands over the xtask binary its warm build already produced and the verifier runs it, which removes about 2.4 minutes from the required run's critical path (#11566).
