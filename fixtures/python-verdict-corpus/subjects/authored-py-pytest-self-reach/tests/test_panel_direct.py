from reach.panel_direct import Panel


def test_panel_padding():
    panel = Panel()
    assert panel._panel_pad() == 2
