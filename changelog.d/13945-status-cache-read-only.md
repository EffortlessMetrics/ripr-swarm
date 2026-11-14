<!-- section: Fixed -->

- `ripr agent status` no longer rewrites or prunes `target/ripr/cache` files while it builds its repair-start inventory, so an attempt whose bound after verdict lists those files stays recoverable instead of refusing. A cold cache only recomputes (#13945).
