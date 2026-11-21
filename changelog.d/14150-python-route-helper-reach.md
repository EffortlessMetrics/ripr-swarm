<!-- section: Fixed -->
- Python: a changed helper that a routed handler directly calls now reads reach-only (`route_helper_call`, weakly exposed) instead of `no_static_path` when a test requests that route through a client identified as the handler's own application; the response assertion earns no oracle credit (#14150).
