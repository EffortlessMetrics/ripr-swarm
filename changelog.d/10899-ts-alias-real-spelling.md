<!-- section: Fixed -->
- TypeScript discovery no longer treats a file that spells an aliased directory's real path (`real/skills` for `link/skills -> ../real/skills`) as an import through the link, so a repository whose own scripts mention that path is not reported partial for a harmless directory alias; an import through the link still is (#10899).
