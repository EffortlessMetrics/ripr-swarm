<!-- section: Fixed -->
- Python: a pytest fixture whose literal `params=` rows reach the test through `request.param` and a leading unpack now reads like the equivalent `@pytest.mark.parametrize` test, so boundary rows credit (`py-fixtures-fee-boundary`). A parametrize mark no longer hides the test's other unresolved fixtures, which had read as `exposed` (#9619).
