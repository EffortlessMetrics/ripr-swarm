<!-- section: Fixed -->
- The Python guard gate again treats `None != 0` as definitely unequal after the bool/numeric literal change in #11842, restoring `compare_literal_values_fails_open_on_undecoded_literals` on main.
