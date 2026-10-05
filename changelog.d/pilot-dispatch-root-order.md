<!-- section: Changed -->
- Cold `ripr pilot` runs on large workspaces are faster. When it looks for
  code that trait dispatch may run, ripr now orders only the functions it has
  not seen yet, and orders them by a precomputed position instead of by path.
  Cold pilot drops from 30.6 s to 27.7 s on bevy, 18.7 s to 17.0 s on
  rust-analyzer and 23.9 s to 22.8 s on nushell, with identical output.
