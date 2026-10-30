from reach.ledger import Wide


def test_render_wide():
    assert Wide().render_ledger() == "|  |"
