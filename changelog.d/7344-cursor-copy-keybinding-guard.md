<!-- section: Fixed -->
- The `ripr: Copy Repair Packet at Cursor` keybinding
  (`Ctrl+Alt+Shift+P` / `Cmd+Alt+Shift+P`) now carries the same ripr
  language guard as its editor/context menu entry, so the cursor-scoped
  command no longer fires in non-routed files such as Markdown or JSON
  (#7344). The workspace-scoped keybindings stay language-agnostic.
