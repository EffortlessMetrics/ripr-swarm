class Ledger:
    def _ledger_pad(self):
        return 2

    def render_ledger(self):
        return "|" + " " * self._ledger_pad() + "|"
