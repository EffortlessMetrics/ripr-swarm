<!-- section: Fixed -->
- A Python boundary edit whose test passes the boundary value through a plain local (`s = 50` then `tier(s)`) is now credited like a literal input, so it is no longer reported as an unplaced input; a name that is rebound, shadowed, a parameter or otherwise not a single literal assignment stays weak (#10707).
