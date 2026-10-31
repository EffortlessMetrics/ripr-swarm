<!-- section: Fixed -->
- A Python boundary edit whose test passes the value through a constant of the test file (`THRESHOLD = 50` above the tests) is now credited like a literal input, so it is no longer reported as an unplaced input; a constant that is bound twice, replaced by `global`, an attribute write or a patch, or shadowed by the test stays weak (#10707).
