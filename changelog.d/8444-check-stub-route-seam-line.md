<!-- section: Fixed -->
- `ripr check` prints the `ripr agent stub --at` line the stub actually targets when the finding line holds no seam of its own, and prints no stub command when that line cannot reproduce the same stub, including for a changed constant routed through a consuming function (#8444).
