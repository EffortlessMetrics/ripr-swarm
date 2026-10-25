<!-- section: Fixed -->
- Python: a changed element on its own line of a multi-line dict or list literal is now judged by the same changed-element rules as a one-line literal, and a changed value counts only as a whole literal token, so an assertion on a sibling key, an unchanged index, or a longer number that contains the value no longer reads `exposed` (#10210).
