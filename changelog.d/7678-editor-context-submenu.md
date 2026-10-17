<!-- section: Fixed -->
- The editor context menu no longer shows eight contiguous ripr entries with
  three near-identical clipboard commands. `Copy Repair Packet at Cursor`
  (already bound to `Ctrl+Alt+Shift+P` / `Cmd+Alt+Shift+P`) stays at the top
  level, and the remaining top-packet copy and inspection commands move behind
  a single **ripr** submenu, so a mis-click no longer silently replaces the
  clipboard and the menu stays glanceable next to rust-analyzer, git, and
  Copilot entries (#7678).
