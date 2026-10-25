<!-- section: Fixed -->
- The staleness watchdog now comments once per head on a merge-ready PR that has no qualification run because it was pushed after the label (or lost its label event), asking the author to remove and re-apply `merge-ready`; before, that case showed only in the job log (#10136).
