from other.models import Table


def test_other_table_renders():
    table = Table()
    assert table.render_table() == "[]"
