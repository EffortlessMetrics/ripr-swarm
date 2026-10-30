from reach.chart_import import Chart


def test_chart_has_render():
    assert hasattr(Chart, "render_chart")
