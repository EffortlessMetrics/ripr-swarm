import sys

from trapkit.core import platform_matched

if sys.platform != "win32":

    def test_platform_matched_total():
        assert platform_matched(3, 7) == 21
