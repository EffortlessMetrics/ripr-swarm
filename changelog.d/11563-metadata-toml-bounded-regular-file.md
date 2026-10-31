<!-- section: Fixed -->
- A hostile `ripr.toml` or `Cargo.toml` made of newlines (well under the 256 MiB
  input cap, but ~27x larger once tokenized) no longer aborts the process on
  allocation, and a FIFO named `ripr.toml` or `Cargo.toml` no longer blocks the
  run forever. Repository metadata TOML is now read only from regular files up
  to 2 MiB; `ripr.toml` over the cap or not a regular file is an exit-2 refusal
  naming the file, and best-effort manifest probes treat such a manifest as
  unresolvable (#11563).
