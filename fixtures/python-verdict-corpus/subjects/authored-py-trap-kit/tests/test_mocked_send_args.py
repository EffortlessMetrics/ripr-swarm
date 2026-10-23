from unittest.mock import Mock

from trapkit.core import mocked_send_args


def test_mocked_send_args():
    sender = Mock()
    mocked_send_args(4, sender)
    sender.send.assert_called_once_with(8)
