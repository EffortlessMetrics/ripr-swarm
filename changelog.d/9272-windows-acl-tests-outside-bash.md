<!-- section: Fixed -->
- Windows tests that need a real directory deny ACL (`doctor` cache probes, the unreadable-subtree sweep) run in a PowerShell step of the sharded workflow, because Git for Windows bash enables backup and restore privileges that bypass the deny (#9272).
