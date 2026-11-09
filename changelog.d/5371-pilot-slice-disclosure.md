fix(pilot): name how the seam-limit slice is chosen (#5371)

The pilot summary's seam-limit line said "ranked N of M seams", but the
pilot budget cuts the classified list BEFORE ranking: it keeps the
inventory's sorted-path prefix plus seams the current change touches, and
ranking reorders only that slice. On a huge repo the analyzed seams are
therefore the lexicographically-first files' seams, not the repo's most
actionable ones — while "ranked" suggested the opposite.

The line now states the selection rule and bounds the claim: the top gap
is the most actionable seam in this slice, not of the whole repository.
Behavior is unchanged; this is disclosure only.
