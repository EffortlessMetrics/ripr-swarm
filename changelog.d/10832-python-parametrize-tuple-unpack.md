<!-- section: Fixed -->
- Python: a parametrized test that unpacks a tuple argname first (`@pytest.mark.parametrize("case", [(50, 5), (500, 0)])` with `amount, fee = case`) now reads its rows as `amount` and `fee` cases, so off-boundary rows report `weakly_exposed` instead of a false `exposed`. A mark that overrides a same-named `params=` fixture reads the mark's rows (#10832).
