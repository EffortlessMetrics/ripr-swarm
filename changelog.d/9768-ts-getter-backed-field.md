<!-- section: Fixed -->
- The TypeScript preview adapter no longer credits a changed field update when
  the owner's class declares a getter or setter for that field: the assertion
  reads through the accessor, so it does not observe what the changed statement
  wrote (#9768).
