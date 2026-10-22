class Table:
    def _table_pad(self):
        return 2

    def render_table(self):
        return "|" + " " * self._table_pad() + "|"
