class Panel:
    def _panel_pad(self):
        return 2

    def render_panel(self):
        return "|" + " " * self._panel_pad() + "|"
