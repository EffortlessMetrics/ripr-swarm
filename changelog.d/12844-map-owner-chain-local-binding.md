<!-- section: Fixed -->
- A test that maps a locally bound closure or nested function named like the changed owner (`let inc = |x| x;` then `.map(inc)`) no longer gets a synthetic owner call fact, so reach is no longer raised on a function the test never runs (#12844).
