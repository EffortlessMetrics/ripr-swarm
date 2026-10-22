<!-- section: Fixed -->
- A returned closure applied inline in an exact pin (`make_adder(2)(3)`) now reads `exposed` when the arguments are distinct nonzero numeric literals; zero, repeated, non-literal and unapplied inputs keep the earlier reading (#9490).
