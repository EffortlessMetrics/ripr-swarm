<!-- section: Changed -->
- The release report's packaged-CLI pilot check accepts the pilot packet file's `run_status` of `packet_bound_applied` (the default packet bound) as a finished run, so the 0.11 release gate does not fail when the fixture has more eligible packets than the bound; `seam_limit_applied`, `limited_declined_files` and any other status still fail (#12065).
