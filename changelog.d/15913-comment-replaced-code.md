<!-- section: Fixed -->
- Python and TypeScript: code replaced by a comment is read as deleted code.
  The diff pairs the added comment with a removed line, and Python reported
  the comment text as a changed `predicate` (`Changed behavior: # ...`) that
  could be graded `exposed`, while TypeScript recorded nothing for the removed
  line. An added run that holds no code now pairs with nothing, so the removed
  line is `base_deleted` evidence naming the removed code, with the class the
  same deletion has when nothing is added. A wrapped import's continuation
  rows and the body rows of a block comment count as no code; a statement
  after a closed block comment keeps its pairing (#15913).
