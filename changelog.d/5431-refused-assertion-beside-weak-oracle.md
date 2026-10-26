<!-- section: Fixed -->
- Rust: a `weakly_exposed` finding no longer asks for a stronger assertion when a related test holds an `assert_eq!` on the owner that ripr could not read (for example a glob import that may rebind the macro) beside the admitted weaker one; it reads `static_unknown` and names the limit (#5431).
