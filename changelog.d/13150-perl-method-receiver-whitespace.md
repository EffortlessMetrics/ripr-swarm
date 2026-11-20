<!-- section: Fixed -->
- Perl threshold checks preserve the method receiver across spaces, newlines,
  and comments after `->`. A call on a different or unknown receiver no longer
  supplies a boundary input for the changed owner; that finding stays
  `static_unknown`, while calls on the known owner retain their credit (#13150).
