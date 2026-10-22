<!-- section: Fixed -->
- A gitfile whose target exists but is not a repository now gets the "not inside a Git work tree" message with newer Git too. Newer Git words this "gitfile does not point to a valid repository", which ripr previously reported as a damaged repository with config-repair advice. An empty `gitdir:` line ("no path in gitfile") is classified the same way (#9661).
