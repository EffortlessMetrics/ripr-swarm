from unittest.mock import MagicMock

from idioms.mailer import send


def test_send_delivers_the_upper_cased_message():
    client = MagicMock()
    send(client, {"email": "a@b.test"}, "hi")
    client.deliver.assert_called_once_with("a@b.test", "HI")


def test_send_skips_users_without_an_email():
    client = MagicMock()
    send(client, {}, "hi")
    client.deliver.assert_not_called()
