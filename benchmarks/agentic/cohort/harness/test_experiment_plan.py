"""Independent invariants for frozen populations, pins and strict input."""
from collections import Counter, defaultdict
from copy import deepcopy
import hashlib
from itertools import groupby, product
import json
from pathlib import Path
import tempfile
import unittest

from experiment_plan import MAX_TRIALS, freeze, parse_json, read_json, validate_plan, write_plan


def sha(value):
    return hashlib.sha256(str(value).encode("utf-8")).hexdigest()


def canonical_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                    allow_nan=False).encode("utf-8")).hexdigest()


def design():
    tasks = []
    for index, partition in enumerate(("development", "holdout")):
        mutations = [sha(f"mutation-{index}-0"), sha(f"mutation-{index}-1")]
        tasks.append(dict(id=f"task-{index}", family=f"family-{index}", partition=partition,
                          stratum="routine", task_sha256=sha(f"task-{index}"), mutants=2,
                          mutant_sha256s=mutations, oracle_sha256=canonical_hash(mutations)))
    return dict(design_schema_version=1, experiment_id="experiment-1", seed=7,
                repetitions=7, max_turns=4, trial_timeout_s=60,
                protocol_sha256=sha("protocol"), evaluator_sha256=sha("evaluator"),
                runner=dict(id="runner-1", environment_sha256=sha("environment")),
                control_arm="baseline", models=[
                    dict(id=f"model-{i}", provider="same-provider", revision=f"revision-{i}",
                         settings_sha256=sha(f"settings-{i}")) for i in range(2)],
                arms=[dict(id=name, configuration_sha256=sha(name),
                           ripr_revision=None if name == "baseline" else "a" * 40,
                           ripr_binary_sha256=None if name == "baseline" else sha(name + "binary"))
                      for name in ("baseline", "candidate", "alternate")], tasks=tasks)


def target(document, path):
    for key in path:
        document = document[key]
    return document


class ExperimentPlanTests(unittest.TestCase):
    def test_duplicate_task_contents_do_not_manufacture_subjects_in_one_partition(self):
        d = design()
        d["tasks"].append(dict(d["tasks"][0], id="duplicate", family="pretend-independent"))
        with self.assertRaisesRegex(ValueError, "duplicate task contents"):
            freeze(d)

    def test_population_is_complete_unique_and_adjacent(self):
        d = design()
        p = freeze(d)
        rows = p["schedule"]
        expected = set(product([x["id"] for x in d["tasks"]], [x["id"] for x in d["models"]],
                               range(1, 8), [x["id"] for x in d["arms"]]))
        self.assertEqual({(r["task"], r["model"], r["rep"], r["arm"]) for r in rows}, expected)
        self.assertEqual(len(rows), len(expected))
        self.assertEqual(len({r["trial_id"] for r in rows}), len(rows))
        self.assertEqual([r["position"] for r in rows], list(range(len(rows))))
        seen_blocks = set()
        for block_id, group in groupby(rows, key=lambda r: r["block_id"]):
            block = list(group)
            self.assertNotIn(block_id, seen_blocks)
            seen_blocks.add(block_id)
            self.assertEqual({r["arm"] for r in block}, {x["id"] for x in d["arms"]})
            self.assertEqual(len({(r["task"], r["model"], r["rep"]) for r in block}), 1)
        self.assertEqual(len(seen_blocks), 2 * 2 * 7)

    def test_arm_positions_balance_for_partial_and_complete_rotations(self):
        for repetitions in (1, 2, 3, 7, 9):
            d = design()
            d["repetitions"] = repetitions
            counts = defaultdict(Counter)
            for _, group in groupby(freeze(d)["schedule"], key=lambda r: r["block_id"]):
                for position, row in enumerate(group):
                    counts[row["task"], row["model"], row["arm"]][position] += 1
            for positions in counts.values():
                values = [positions[index] for index in range(3)]
                self.assertLessEqual(max(values) - min(values), 1)
                self.assertEqual(sum(values), repetitions)

    def test_reproducible_order_invariant_detached_and_seed_sensitive(self):
        d = design()
        original = deepcopy(d)
        expected = freeze(d)
        self.assertEqual(d, original)
        self.assertEqual(freeze(d), expected)
        for key in ("models", "arms", "tasks"):
            d[key].reverse()
        self.assertEqual(freeze(d), expected)
        self.assertEqual(validate_plan(expected), expected)
        d["seed"] += 1
        changed = freeze(d)
        self.assertNotEqual(changed["design_sha256"], expected["design_sha256"])
        order = lambda p: [(r["task"], r["model"], r["rep"], r["arm"]) for r in p["schedule"]]
        self.assertNotEqual(order(changed), order(expected))
        expected["design"]["runner"]["id"] = "changed"
        self.assertEqual(original["runner"]["id"], "runner-1")

    def test_declared_pins_bind_hashes_and_rows(self):
        p = freeze(design())
        self.assertEqual(p["design_sha256"], canonical_hash(p["design"]))
        self.assertEqual(p["plan_sha256"], canonical_hash({k: v for k, v in p.items() if k != "plan_sha256"}))
        for path, key in (((), "protocol_sha256"), ((), "evaluator_sha256"),
                          (("runner",), "environment_sha256"), (("models", 0), "settings_sha256"),
                          (("arms", 1), "ripr_binary_sha256"), (("tasks", 0), "task_sha256")):
            d = design()
            target(d, path)[key] = sha("changed")
            changed = freeze(d)
            self.assertNotEqual(changed["design_sha256"], p["design_sha256"])
            self.assertTrue({r["trial_id"] for r in changed["schedule"]}.isdisjoint(
                r["trial_id"] for r in p["schedule"]))

    def test_missing_and_unknown_fields_reject_at_every_design_level(self):
        for path in ((), ("runner",), ("models", 0), ("arms", 0), ("tasks", 0)):
            for key in target(design(), path):
                d = design()
                del target(d, path)[key]
                with self.subTest(path=path, missing=key), self.assertRaises(ValueError):
                    freeze(d)
            d = design()
            target(d, path)["unrecognized"] = 1
            with self.subTest(path=path), self.assertRaises(ValueError):
                freeze(d)

    def test_malformed_numbers_and_identifiers_reject(self):
        for key in ("seed", "repetitions", "max_turns", "trial_timeout_s", "design_schema_version"):
            for value in (True, 1.0, "1", None, -1):
                d = design()
                d[key] = value
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    freeze(d)
        for path, key in (((), "experiment_id"), ((), "control_arm"), (("runner",), "id"),
                          (("models", 0), "id"), (("arms", 0), "id"), (("tasks", 0), "id"),
                          (("tasks", 0), "family")):
            for value in ("", "  ", "bad id", "bad\n", "bad\x00", "bad\x7f", "bad\u200b", None, 1):
                d = design()
                target(d, path)[key] = value
                with self.subTest(path=path, key=key, value=value), self.assertRaises(ValueError):
                    freeze(d)
        for key in ("provider", "revision"):
            for value in ("", " ", None, 1):
                d = design()
                d["models"][0][key] = value
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    freeze(d)

    def test_collections_duplicates_versions_and_bounds_reject(self):
        for key in ("tasks", "models", "arms"):
            for value in (None, {}, [], "bad"):
                d = design()
                d[key] = value
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    freeze(d)
            d = design()
            d[key].append(deepcopy(d[key][0]))
            with self.subTest(duplicate=key), self.assertRaises(ValueError):
                freeze(d)
        for key, value in (("repetitions", 0), ("max_turns", 0), ("trial_timeout_s", 0),
                           ("design_schema_version", 2), ("repetitions", MAX_TRIALS),
                           ("control_arm", "missing"), ("arms", design()["arms"][:1])):
            d = design()
            d[key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                freeze(d)

    def test_hashes_binary_pairs_and_canonical_arm_names_reject(self):
        for value in ("a" * 63, "g" * 64, "a" * 65, None, True):
            d = design()
            d["protocol_sha256"] = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                freeze(d)
        for key, value in (("ripr_revision", "a" * 7), ("ripr_revision", "A" * 40),
                           ("ripr_revision", None), ("ripr_binary_sha256", None),
                           ("id", "A0-none"), ("id", "A1-diagnostics"), ("id", "A2-packet-loop")):
            d = design()
            d["arms"][1][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                freeze(d)
        d = design()
        d["protocol_sha256"] = d["protocol_sha256"].upper()
        self.assertEqual(freeze(d), freeze(design()))
        d.update(seed=0, repetitions=1, models=d["models"][:1], arms=d["arms"][:2])
        self.assertEqual(len(freeze(d)["schedule"]), 4)

    def test_partition_leaks_and_invalid_task_declarations_reject(self):
        for key in ("family", "task_sha256"):
            d = design()
            d["tasks"][1][key] = d["tasks"][0][key]
            with self.subTest(key=key), self.assertRaises(ValueError):
                freeze(d)
        for key, value in (("partition", "development"), ("partition", "invalid"),
                           ("stratum", "invalid"), ("mutants", 0), ("mutants", True),
                           ("mutants", 2.0), ("mutant_sha256s", []), ("mutant_sha256s", None),
                           ("mutant_sha256s", [sha(1)] * 2), ("oracle_sha256", sha("wrong"))):
            d = design()
            d["tasks"][1][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                freeze(d)
        d = design()
        d["tasks"][0]["mutant_sha256s"].reverse()
        with self.assertRaises(ValueError):
            freeze(d)

    def test_edited_plan_schedule_and_loose_integer_equality_reject(self):
        p = freeze(design())
        for path in ((), ("schedule", 0)):
            for key in target(p, path):
                changed = deepcopy(p)
                del target(changed, path)[key]
                with self.subTest(path=path, missing=key), self.assertRaises(ValueError):
                    validate_plan(changed)
            changed = deepcopy(p)
            target(changed, path)["extra"] = 0
            with self.assertRaises(ValueError):
                validate_plan(changed)
        for path, key, value in (((), "plan_schema_version", True), ((), "plan_schema_version", 2),
                                 ((), "design_sha256", sha("edited")), ((), "plan_sha256", sha("edited")),
                                 (("schedule", 0), "rep", True), (("schedule", 0), "position", False),
                                 (("schedule", 0), "rep", float(p["schedule"][0]["rep"])), (("schedule", 0), "arm", "missing"),
                                 (("schedule", 0), "trial_id", sha("substituted"))):
            changed = deepcopy(p)
            target(changed, path)[key] = value
            with self.subTest(path=path, key=key, value=value), self.assertRaises(ValueError):
                validate_plan(changed)
        for rows in (p["schedule"][:-1], list(reversed(p["schedule"])), [p["schedule"][0]] * len(p["schedule"])):
            changed = deepcopy(p)
            changed["schedule"] = rows
            changed["plan_sha256"] = canonical_hash({k: v for k, v in changed.items() if k != "plan_sha256"})
            with self.assertRaises(ValueError):
                validate_plan(changed)

    def test_strict_json_decoder_and_exclusive_write(self):
        for text in ('{"id": 1, "id": 2}', '{"nested": {"id": 1, "id": 2}}',
                     '{"x": NaN}', '{"x": Infinity}', '{"x": -Infinity}', '{"x": 1e999}',
                     '{"x": [1, -1e999]}', '[]', 'null', '{'):
            with self.subTest(text=text), self.assertRaises(ValueError):
                parse_json(text)
        self.assertEqual(parse_json('{"finite": 1.5}'), {"finite": 1.5})
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "plan.json"
            p = freeze(design())
            write_plan(path, p)
            self.assertEqual(read_json(path), p)
            original = path.read_bytes()
            with self.assertRaises(FileExistsError):
                write_plan(path, p)
            self.assertEqual(path.read_bytes(), original)
            invalid = Path(directory) / "invalid.json"
            with self.assertRaises(ValueError):
                write_plan(invalid, {})
            self.assertFalse(invalid.exists())
            path.write_text('{"x": 1, "x": 2}')
            with self.assertRaises(ValueError):
                read_json(path)


if __name__ == "__main__":
    unittest.main()
