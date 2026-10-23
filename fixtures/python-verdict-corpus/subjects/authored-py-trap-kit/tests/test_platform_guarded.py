import sys

from trapkit.core import platform_guarded

if sys.platform == "win32":

    def test_platform_guarded_total():
        assert platform_guarded(3, 7) == 21
