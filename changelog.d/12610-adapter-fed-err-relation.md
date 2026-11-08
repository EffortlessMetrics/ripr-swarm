<!-- section: Fixed -->
- An assertion whose `Err(..)` the test hands to a value-supplying adapter (`or`, `ok_or`, `map_err`, ..) is now a weak relation instead of an exact value, so a boundary row on the owner no longer takes `discriminate: yes` from a test that passes whatever the owner returns (#12610).
