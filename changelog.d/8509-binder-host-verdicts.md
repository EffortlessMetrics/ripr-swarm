<!-- section: Fixed -->
- The LSP portable-command binder now withholds a route carrying the bash apostrophe escape `'\''` on every host: no Windows console reads that quoting construct, and the token scan cannot follow it, so binding it corrupted the quoted data and left the real `--root .` unbound. A plain separator backslash still binds on Windows (#8509, #8153).
