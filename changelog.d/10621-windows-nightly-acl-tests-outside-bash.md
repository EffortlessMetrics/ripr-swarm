<!-- section: Fixed -->
- The nightly Windows advisory lane runs the four tests that need a real directory deny ACL (`doctor` cache probes, the unreadable-subtree sweep) in a PowerShell step with its own log and status, and the bash test steps skip them, so the nightly no longer reports them as `NOT_ESTABLISHED` (#10621).
