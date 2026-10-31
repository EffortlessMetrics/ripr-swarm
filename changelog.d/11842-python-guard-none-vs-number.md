<!-- section: Fixed -->
- The Python guard gate again treats `None != 0` as definitely unequal after the shared literal equality became three-valued in #11842; the boundary rule's equality stays unknown for that pair, restoring `compare_literal_values_fails_open_on_undecoded_literals` on main.
