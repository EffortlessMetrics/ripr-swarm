<!-- section: Fixed -->
- LSP: a clean tree or empty analysis scope no longer leaves the editor on an
  empty diagnostic list that reads as an all-clear. When the snapshot's
  outcome is an empty scope and it holds no row of any kind, each document the
  client asks about carries one passive Information marker under the existing
  `ripr-diagnostic-language-not-analyzed` code (`data.kind` `empty_scope`)
  saying ripr found no changed behavior in the analyzed scope, that this is a
  static advisory, not an all-clear and not runtime evidence, with the
  set-a-base route. Push clients get it when the tree becomes clean and have
  it withdrawn when real rows appear. It carries no finding identity, repair
  data or actionability, is never a Warning, and is absent whenever the
  snapshot has findings (#13441).
