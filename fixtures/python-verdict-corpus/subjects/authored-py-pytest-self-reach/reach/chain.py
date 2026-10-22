class Chain:
    def run(self):
        return self._a()

    def _a(self):
        return self._b()

    def _b(self):
        return self._c()

    def _c(self):
        return self._d()

    def _d(self):
        return self._e()

    def _e(self):
        return self._f()

    def _f(self):
        return 6
