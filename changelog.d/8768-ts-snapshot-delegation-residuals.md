<!-- section: Fixed -->
- TypeScript committed-snapshot credit now also refuses a Jest/Vitest
  config that names `projects`, `workspace`, `root` or `update` as a
  computed key (`["projects"]:`) or a member assignment
  (`config.projects = ...`), and a `package.json` script that puts `-u`
  after a JSON-escaped whitespace (`"jest\t-u"`). A submodule's `.git`
  pointer no longer ends the above-root config walk, so the
  superproject's runner config is read. A comment between a key and its
  `:` / `=`, compound member assignments (`||=`, `??=`, `+=`), a shell-quoted
  or bundled `-u` (`'-u'`, `-iu`) and JSON-escaped whitespace around a config
  flag no longer hide the override (#8768).
