<!-- section: Internal / CI -->

- `cargo xtask warm-ab --control-slowdown <ratio>` scales the negative control to each pin's own baseline, so the recorded control can fail every pin (the fixed 5 s sleep failed only the two slow-baseline external pins), and the packet records `control_slowdown` (#12244).
