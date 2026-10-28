<!-- section: Fixed -->
- TypeScript barrel discovery withholds test credit when a resolved star
  source cannot be indexed, instead of treating its missing export facts as
  an empty module and promoting a sibling binding. Missing-link guidance
  asks users to inspect existing calls and imports before adding a test;
  unavailable evidence does not establish that no test exists (#4502).
