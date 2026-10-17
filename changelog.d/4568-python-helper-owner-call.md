<!-- section: Fixed -->
- Python: a private helper reached only through a same-module caller that a test calls is now `weakly_exposed` with a `helper_owner_call` relation instead of `no_static_path` (#4568); the relation carries no oracle.
