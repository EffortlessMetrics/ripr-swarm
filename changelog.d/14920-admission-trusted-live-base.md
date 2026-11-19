<!-- section: Fixed -->
- Candidate admission read the enforcement packet, the generated-path
  declaration and the base commit from a second live base-tip read, while the
  trusted bundle was checked out at the commit the workflow resolved. When the
  base advanced after the merge ref was built, one commit's rules judged another
  commit's packet and declaration. The admit step now hands that commit in on
  every arm, and the action reads all three from it, so the rule set, the packet
  and the declaration are one identity by construction (#14932).
