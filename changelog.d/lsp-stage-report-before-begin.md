<!-- section: Fixed -->
- LSP: a stage report that arrives before the client has answered the
  work-done progress create is now held and sent right after the begin, in
  order, instead of being dropped, so the "loading input" stage no longer goes
  missing from a fast refresh (#9362).
