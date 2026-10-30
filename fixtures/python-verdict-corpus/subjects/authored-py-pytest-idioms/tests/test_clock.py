from unittest.mock import patch

from idioms.clock import stamp


def test_stamp_uses_the_patched_clock():
    with patch("idioms.clock.time.time", return_value=1700000000):
        assert stamp("a") == "a-1700000000"
