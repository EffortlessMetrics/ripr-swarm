<!-- section: Fixed -->
- A free-function owner reached only through an unpolled `async fn` hop no longer earns `helper_owner_call` credit: the helper-chain truncation at the first non-synchronous hop now applies to every owner, not only inherent-method owners, matching the compact helper-path refusal (#7415, #11151).
