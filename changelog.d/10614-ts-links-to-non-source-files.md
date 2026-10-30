<!-- section: Fixed -->
- TypeScript analysis no longer reports `typescript_workspace_links_skipped` for symlinks that point at non-source files (docs, configs, assets). Links that could hide TypeScript sources, including directory links, dangling links and links to source files, are still disclosed (#10614).
