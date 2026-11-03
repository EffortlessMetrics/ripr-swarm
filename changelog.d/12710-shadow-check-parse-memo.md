<!-- section: Fixed -->
- The Rust cold pilot on large workspaces no longer re-parses the same test body for every owner-shadow check, which had roughly doubled its time since #10167 (#12710).
