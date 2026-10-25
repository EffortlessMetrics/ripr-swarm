<!-- section: Fixed -->
- Python: a fixture declared through an aliased pytest import (`from pytest import fixture as fx`) is now recorded like `@pytest.fixture`, so a `caplog` override written that way abstains instead of being credited and aliased fixtures count for fixture reach (#10454).
