- Replace the repository-policy timeout test's naturally expiring descendant
  with a FIFO-blocked witness. Liveness now determines termination independently
  of observation delay; bounded cleanup reaps the owned tree, and primary-only
  signaling controls must fail the liveness assertion.
