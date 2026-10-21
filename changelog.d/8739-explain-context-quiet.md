<!-- section: Fixed -->
- `ripr explain` and `ripr context` accept `--quiet` with the `ripr check` meaning (suppress the stderr progress stream only; stdout, exit codes and errors unchanged), so an agent can reuse `check --json --quiet` flags down the drill-in chain instead of hitting an exit-2 usage error (#8739).
