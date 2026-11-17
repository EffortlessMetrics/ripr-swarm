<!-- section: Fixed -->
- A private helper that its caller calls at several sites (for example
  `number(parts[0])?`, `number(parts[1])?`, `number(parts[2])?` inside
  `parse`) is no longer reported as having no test path when a test calls
  `parse`. ripr now names the related test and reads the change as reached.
  It still does not borrow one call's inputs for the others, so the result
  stays an honest "unclear" until a test passes inputs ripr can see (#14918).
