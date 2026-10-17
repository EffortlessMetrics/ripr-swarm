<!-- section: Fixed -->
- The LSP `initialize` advertisement no longer lists `quickfix.ripr` and
  `quickfix.ripr.verify` in `CodeActionOptions.code_action_kinds`: no code
  action emitter ever produced those kinds, so clients that filter, keybind,
  or group on the advertised set got permanent silence for two of the five
  promised kinds. The advertised set is now exactly the emitted set
  (`quickfix.ripr.inspect`, `quickfix.ripr.navigate`,
  `source.ripr.refresh`), the parity tests pin the invariant in both
  directions (emitted ⊆ advertised and advertised ⊆ emitted), and the
  reserved kinds can re-enter the advertisement once a real emitter lands
  without breaking clients (#7773).
