import unittest

from guards.errors import check_b, check_d


class TestGuards(unittest.TestCase):
    def test_callable_type_only(self):
        self.assertRaises(ValueError, check_b, "")

    def test_bound_exception_equal(self):
        with self.assertRaises(ValueError) as ctx:
            check_d("")
        self.assertEqual(str(ctx.exception), "empty")
