"""Native Python controls for #9815; no third-party test runner required."""
import ast
import copy
import os
from pathlib import Path
import unittest


def old_product(a, b):
    return a * b


def reordered_product(a, b):
    return b * a


class OrderedProduct:
    def __init__(self, label):
        self.label = label

    def __mul__(self, other):
        return (self.label, getattr(other, "label", other))

    def __rmul__(self, other):
        return (getattr(other, "label", other), self.label)


class FactorReorderControls(unittest.TestCase):
    def test_annotations_do_not_enforce_numeric_operands(self):
        for numeric in (int, float):
            def annotated(a: numeric, b: numeric):
                return a * b
            def swapped(a: numeric, b: numeric):
                return b * a
            with self.subTest(annotation=numeric.__name__):
                b = OrderedProduct("b")
                self.assertEqual(annotated(3, b), (3, "b"))
                self.assertEqual(swapped(3, b), ("b", 3))
                self.assertNotEqual(annotated(3, b), swapped(3, b))

    def test_unchanged_corpus_oracles(self):
        repository = Path(os.environ.get("RIPR_SOURCE_ROOT", Path(__file__).resolve().parents[2]))
        root = repository / "fixtures/python-verdict-corpus/subjects"
        subjects = [
            ("authored-py-trap-kit", "trapkit/core.py", "tests/test_filled_table.py", "filled_table", "test_filled_table"),
            ("authored-py-grid-arithmetic", "grid_arithmetic/core.py", "tests/test_core.py", "area", "test_area_table"),
        ]
        for subject, source, tests, name, test_name in subjects:
            with self.subTest(subject=subject):
                source_path = root / subject / source
                test_path = root / subject / tests
                function = next(n for n in ast.parse(source_path.read_text()).body
                                if isinstance(n, ast.FunctionDef) and n.name == name)
                test = next(n for n in ast.parse(test_path.read_text()).body
                            if isinstance(n, ast.FunctionDef) and n.name == test_name)
                rows = ast.literal_eval(test.decorator_list[0].args[1])
                self.assertGreater(len(rows), 0)
                # Preserve the actual assertion and literal rows. Remove only
                # pytest's decorator/import dependency; this is a stdlib replay,
                # not a claim that pytest executed in this environment.
                test.decorator_list = []
                namespace = {}
                exec(compile(ast.Module(body=[test], type_ignores=[]), str(test_path), "exec"), namespace)
                counts = {}
                for variant in ("baseline", "swapped", "addition"):
                    candidate = copy.deepcopy(function)
                    product = candidate.body[0].value
                    self.assertIsInstance(product, ast.BinOp)
                    self.assertIsInstance(product.op, ast.Mult)
                    if variant == "swapped":
                        product.left, product.right = product.right, product.left
                    if variant == "addition":
                        product.op = ast.Add()
                    exec(compile(ast.fix_missing_locations(ast.Module(body=[candidate], type_ignores=[])), str(source_path), "exec"), namespace)
                    passed = 0
                    for row in rows:
                        try:
                            namespace[test_name](*row)
                            passed += 1
                        except AssertionError:
                            pass
                    counts[variant] = passed
                self.assertEqual(counts, {"baseline": len(rows), "swapped": len(rows), "addition": 0})
                print(f"{name}: original_rows={len(rows)} baseline_pass={counts['baseline']} "
                      f"swapped_pass={counts['swapped']} addition_rejected={len(rows)-counts['addition']}")

    def test_numeric_controls(self):
        for a, b in [(3, 7), (-4, 2), (1.5, 2.0)]:
            with self.subTest(a=a, b=b):
                self.assertEqual(old_product(a, b), reordered_product(a, b))

    def test_custom_mul_discriminates(self):
        a, b = OrderedProduct("a"), OrderedProduct("b")
        self.assertEqual(old_product(a, b), ("a", "b"))
        self.assertEqual(reordered_product(a, b), ("b", "a"))
        self.assertNotEqual(old_product(a, b), reordered_product(a, b))

    def test_custom_rmul_discriminates(self):
        a, b = 3, OrderedProduct("b")
        self.assertEqual(old_product(a, b), (3, "b"))
        self.assertEqual(reordered_product(a, b), ("b", 3))
        self.assertNotEqual(old_product(a, b), reordered_product(a, b))


if __name__ == "__main__":
    unittest.main()
