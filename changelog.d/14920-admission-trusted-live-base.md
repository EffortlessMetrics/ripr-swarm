<!-- section: Fixed -->
- Candidate admission read the enforcement packet and the generated-path
  declaration from a second live base-tip read, while the trusted bundle was
  checked out at the commit the workflow resolved. When the base advanced after
  the `pull_request` merge ref was built, one commit's rules judged another
  commit's packet and declaration. The action now reads both from the commit the
  bundle was checked out at, so the rule set, the packet and the declaration are
  one identity by construction (#14932).
