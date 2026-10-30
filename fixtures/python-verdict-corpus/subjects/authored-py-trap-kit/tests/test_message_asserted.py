from trapkit.core import message_asserted


def test_message_asserted():
    assert message_asserted(3, 7) == 21, "three sevens"
