<!-- section: Fixed -->
- TypeScript discovery no longer reports a directory link as a skipped link when it aliases a directory inside the repository that the walk already visits and no indexed file names the link as a path component (`.tools/skills -> ../.agent/skills`); links leaving the root, into excluded directories, or dangling are still disclosed (#10829).
