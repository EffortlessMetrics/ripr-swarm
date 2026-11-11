<!-- section: Fixed -->
- Editor hover, related information and the repair card no longer name an
  unrelated test as the fix site. A test linked to the change only by
  proximity (same file, same module, a token substring) used to win over a
  test that calls the changed function whenever it had an assertion, so ripr
  pointed at an assertion on other code. The fix site, the code-lens click and
  the seam "open best related test" action now skip proximity-only tests; with
  no other related test the fix site is reported unavailable and nothing
  navigates. Equally ranked tests resolve to the first one listed. Python and
  Perl tests their producers declare as the repair target keep that target
  (#5354).
