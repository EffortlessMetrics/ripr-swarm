<!-- section: Fixed -->
- The `ripr agent repair --phase after` refusal for a gap with several
  repairs in progress lists the attempt ids in sorted order, so the same
  store prints the same message on every filesystem instead of following
  directory listing order (#14103).
