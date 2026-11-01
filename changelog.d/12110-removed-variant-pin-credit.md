<!-- section: Fixed -->
- An exact assertion that pins the error variant a changed match arm replaced, through an owner call whose input selects that arm, now credits the `error_path` change instead of leaving it `weakly_exposed`; a pin reached through another arm or a file-proximity test is still not credited (#12110).
