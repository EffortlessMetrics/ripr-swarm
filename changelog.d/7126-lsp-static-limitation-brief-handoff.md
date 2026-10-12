<!-- section: Fixed -->
- LSP hover and code actions no longer advertise `ripr agent brief` for
  static-limitation seams (`opaque` and the `*_unknown` classes). The brief
  command already refuses those classes with an empty brief and a named
  omission warning, so the editor now omits the `- brief:` handoff line and
  the copy-agent-brief action instead of rerouting or annotating a command
  that will not populate. The existing `- packet:` handoff stays, and hover
  next-step uses `is_static_limitation()` so unknown-class seams advise
  inspecting the static limitation rather than adding an assertion
  ([#7126](https://github.com/EffortlessMetrics/ripr-swarm/issues/7126)).
