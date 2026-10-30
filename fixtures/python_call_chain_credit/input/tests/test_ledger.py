from reach.ledger import Ledger


def test_render_ledger():
    ledger = Ledger()
    assert ledger.render_ledger() == "|  |"
