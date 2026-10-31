<!-- section: Fixed -->
- Repair attempt currentness now notices changed contents or index state on an
  already-bound test or source path instead of relying on the path and change
  kind alone. Status reports unknown currentness for legacy path-only bindings;
  receipt reissue refuses them and requires a fresh before phase (#11005).
- Shared workflow writes retain and remove only transaction-owned backups;
  producer cleanup no longer immediately invalidates a newly finished attempt.
  Finishing another attempt can still invalidate a bound manifest identity.
  Missing repair roots are rejected before workflow lock acquisition (#11092).
