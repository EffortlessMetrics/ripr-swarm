<!-- section: Fixed -->
- The language server no longer advertises the `quickfix.ripr.verify` code-action kind, which no action ever used. `quickfix.ripr` stays advertised as the hierarchy root: a request for it keeps every `quickfix.ripr.*` action (#7773).
