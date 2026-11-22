<!-- section: Fixed -->
- A related test's `oracle_semantics.upgrade_suggestion` for a field write in a method (`self.count += 1`) no longer tells you to assert "the specific output field" of a returned struct: it says to assert the changed field on the value the method was called on (a `&mut self` owner), or on the value the call returns (a by-value `self` owner) (#11726).
