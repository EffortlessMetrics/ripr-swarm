<!-- section: Fixed -->
- The VS Code `ripr.server.path` setting now accepts a leading `~` for the home directory
  (for example `~/.cargo/bin/ripr`), and a relative path such as `./target/debug/ripr` is
  refused with a message that asks for the full path instead of a generic "could not start".
