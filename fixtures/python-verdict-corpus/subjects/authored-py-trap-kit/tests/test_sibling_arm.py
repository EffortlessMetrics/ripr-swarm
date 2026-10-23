from trapkit.core import sibling_arm


def test_bulk_rate():
    assert sibling_arm("bulk", 4) == 12
