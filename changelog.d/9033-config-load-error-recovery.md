<!-- section: Changed -->
- A `ripr.toml` that fails to load now ends its error with one line naming the way out (fix the error above, or remove `ripr.toml` to use the built-in defaults) and no longer leaves a stray blank line after the TOML excerpt. The one-line JSON refusal `message` and `doctor` evidence are unchanged (#9033).
