<!-- section: Fixed -->
- TypeScript committed-snapshot credit now also refuses a Jest/Vitest
  config that names `projects`, `workspace`, `root` or `update` as a
  computed key (`["projects"]:`) or a member assignment
  (`config.projects = ...`), and a `package.json` script that puts `-u`
  after a JSON-escaped whitespace (`"jest\t-u"`). A submodule's `.git`
  pointer no longer ends the above-root config walk, so the
  superproject's runner config is read (#8768).
