import pytest

from vers.parse import InvalidVersion, parse_strict, parse_version


def test_parse_version_rejects_blank():
    with pytest.raises(ValueError, match="invalid version"):
        parse_version(" ")


def test_parse_strict_rejects_blank():
    with pytest.raises(InvalidVersion, match="invalid version"):
        parse_strict(" ")
