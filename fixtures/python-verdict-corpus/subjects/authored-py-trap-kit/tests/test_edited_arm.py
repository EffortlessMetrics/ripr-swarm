from trapkit.core import edited_arm


def test_single_rate():
    assert edited_arm("single", 5) == 10
