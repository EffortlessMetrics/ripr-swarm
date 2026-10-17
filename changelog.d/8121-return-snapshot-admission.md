<!-- section: Fixed -->
- Repository return-value grading no longer lets a snapshot of an unrelated
  value in the same test close a changed return seam. A snapshot discriminates
  the seam only when it names the owner call or a plain local bound to it;
  independent exact assertions and direct owner snapshots keep their credit
  (#8121).
