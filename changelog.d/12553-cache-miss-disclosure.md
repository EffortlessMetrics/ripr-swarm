<!-- section: Changed -->
- Repo-wide runs that read the seam-facts cache now say on stderr when the cache has no usable entry and the full walk will run, and the audit-path notice only promises faster warm reruns when a stored cache entry exists; otherwise it says the first run on the workspace pays the full walk (#12505, #12553).
