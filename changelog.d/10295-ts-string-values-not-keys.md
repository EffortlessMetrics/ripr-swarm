<!-- section: Fixed -->
- A quoted string value such as `displayName: "projects"` or `name: "root"` in a Jest or Vitest config is no longer read as a `projects`, `workspace` or `root` key, so a snapshot test that catches the edit stops reading `weakly_exposed` because of a label (#10295). A real second key, a quoted key and a computed key still refuse.
