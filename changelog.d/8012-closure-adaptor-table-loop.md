<!-- section: Fixed -->
- The closure-adaptor run check reads a constant-row table column only for a call inside that table's loop body, so a same-named item outside the loop no longer credits the closure as run (#8012).
