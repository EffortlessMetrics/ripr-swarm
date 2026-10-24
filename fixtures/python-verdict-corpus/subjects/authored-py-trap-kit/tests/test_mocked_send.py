from unittest.mock import Mock

from trapkit.core import mocked_send


def test_mocked_send():
    sender = Mock()
    mocked_send(4, sender)
    sender.send.assert_called()
