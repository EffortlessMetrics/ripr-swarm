class Chart:
    def _chart_pad(self):
        return 2

    def render_chart(self):
        return "|" + " " * self._chart_pad() + "|"
