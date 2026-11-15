<!-- section: Fixed -->
- Python repair: `ripr continue` no longer reports a gap closed when the new test's only crediting assertion pins the old raised type (`pytest.raises(OldExc, ...)` after the raise changed to `NewExc`); it says the test will fail against the new raise and names the next step. Exposure classification is unchanged (#13622).
