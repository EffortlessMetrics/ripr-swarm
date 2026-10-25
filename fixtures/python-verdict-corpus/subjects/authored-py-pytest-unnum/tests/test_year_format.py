from unnum.year_format import format_year


def test_format_year_pads_the_last_two_digits():
    assert format_year(2024) == "024"
