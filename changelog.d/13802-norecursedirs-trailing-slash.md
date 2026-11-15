<!-- section: Fixed -->
- Python collection config: a pytest `norecursedirs` entry written with one trailing slash (`env/`, `.pyenv/`) is read as the directory name instead of marking the whole configuration unevaluated; other `/`-bearing entries stay unevaluated (#13802).
