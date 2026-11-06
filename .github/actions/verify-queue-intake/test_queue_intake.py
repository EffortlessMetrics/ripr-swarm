"""Hostile fixture controls for the source-only H-intake validator."""
from copy import deepcopy
from datetime import datetime, timezone
import json
import unittest

import queue_intake as q

H = "a" * 40
BASE = "c" * 40
GEN = "b" * 64
NOW = datetime(2026, 11, 6, tzinfo=timezone.utc)
BINDING = q.Binding("EffortlessMetrics/ripr-swarm", 42, H, "entry-42", 123456,
                    "Reviewed Publisher", "CI Lifecycle Qualification", GEN,
                    {".github": {"kind": "tree", "sha": "e" * 40, "mode": "040000"},
                     "scripts/control.py": {"kind": "blob", "sha": "f" * 40, "mode": "100644"}})


def payload():
    return {"schema_id": "hermes.queue-intake.v2", "schema_version": 2,
            "repository": BINDING.repository, "pull_number": 42, "head_sha": H,
            "entry_id": "entry-42", "admission_request_id": "admission-1",
            "enqueue_request_id": "enqueue-1", "base_sha": BASE,
            "expires_at": "2026-11-07T00:00:00Z", "authority_sha256": "d" * 64,
            "producer_generation_sha256": GEN,
            "producer_closure": {".github": {"kind": "tree", "sha": "e" * 40, "mode": "040000"},
                                 "scripts/control.py": {"kind": "blob", "sha": "f" * 40, "mode": "100644"}}}


def check(data=None):
    return {"id": 71, "name": BINDING.publisher_check_name, "head_sha": H,
            "external_id": "hermes-queue:admission-1:H", "status": "completed", "conclusion": "success",
            "app": {"id": BINDING.publisher_app_id, "name": BINDING.publisher_app_name},
            "output": {"text": json.dumps(payload() if data is None else data)}}


def current():
    return {"repository": BINDING.repository, "pull_number": 42, "head_sha": H,
            "entry_id": "entry-42", "head_repository": BINDING.repository,
            "base_ref": "main", "state": "OPEN", "draft": False}


def listing(checks=None, total=None):
    rows = [check()] if checks is None else checks
    return json.dumps({"total_count": len(rows) if total is None else total, "check_runs": rows})


def run(observations=None, currents=None, times=None, binding=BINDING):
    observations = iter(observations or [listing(), listing()])
    currents = iter(currents or [json.dumps(current())] * 3)
    times = iter(times or [NOW, NOW, NOW])
    paths = []
    def read_checks(path):
        paths.append(path)
        return next(observations)
    result = q.observe(binding, read_checks, lambda: next(currents), lambda: next(times), lambda: None)
    return result, paths


class IntakeTests(unittest.TestCase):
    def test_exact_h_and_entry_are_reobserved_without_equating_admission_and_group_base(self):
        result, paths = run()
        self.assertEqual(result["intake"]["base_sha"], BASE)
        self.assertEqual(result["check_run_id"], 71)
        self.assertEqual(result["status"], "VALID_AGAINST_SUPPLIED_OBSERVATIONS")
        self.assertEqual(len(paths), 2)
        self.assertEqual(paths[0], paths[1])
        self.assertIn(f"/commits/{H}/check-runs?", paths[0])
        self.assertIn("filter=all", paths[0])
        self.assertIn("per_page=100", paths[0])

    def test_wrong_head_entry_repository_and_generation_refuse(self):
        for key, value in [("head_sha", "1" * 40), ("entry_id", "entry-other"),
                           ("repository", "EffortlessMetrics/other"),
                           ("producer_generation_sha256", "2" * 64)]:
            with self.subTest(key=key):
                data = payload()
                data[key] = value
                with self.assertRaises(q.Refused):
                    run([listing([check(data)]), listing([check(data)])])

    def test_incomplete_duplicate_or_ambiguous_check_observation_refuses(self):
        for response in [listing([], 1), listing([check()], 2), listing([check(), check()]),
                         '{"total_count":1,"total_count":1,"check_runs":[]}',
                         '{"total_count":NaN,"check_runs":[]}']:
            with self.subTest(response=response[:70]):
                with self.assertRaises(q.Refused):
                    run([response, response])

    def test_check_origin_and_conclusion_are_required(self):
        for mutate in [lambda c: c["app"].update(id=15368),
                       lambda c: c["app"].update(id=float(BINDING.publisher_app_id)),
                       lambda c: c["app"].update(name="Other App"),
                       lambda c: c.update(name="Other check"),
                       lambda c: c.update(head_sha="1" * 40),
                       lambda c: c.update(external_id="wrong"),
                       lambda c: c.update(status="in_progress"),
                       lambda c: c.update(conclusion="failure"),
                       lambda c: c["output"].update(text=None)]:
            row = check()
            mutate(row)
            with self.subTest(row=row):
                with self.assertRaises(q.Refused):
                    run([listing([row]), listing([row])])

    def test_duplicate_payload_fields_nonfinite_extra_or_legacy_schema_refuse(self):
        raw = json.dumps(payload())
        duplicate = raw[:-1] + ', "head_sha": "' + H + '"}'
        nonfinite = raw[:-1] + ', "bad": NaN}'
        for value in [duplicate, nonfinite, json.dumps({**payload(), "extra": 1}),
                      json.dumps({**payload(), "schema_version": 1})]:
            row = check()
            row["output"]["text"] = value
            with self.assertRaises(q.Refused):
                run([listing([row]), listing([row])])

    def test_expiry_and_fresh_clock_are_required(self):
        for expiry in ["2026-11-06T00:00:00Z", "2026-11-07T00:00:00", "invalid"]:
            data = payload()
            data["expires_at"] = expiry
            with self.assertRaises(q.Refused):
                run([listing([check(data)]), listing([check(data)])])
        expiry = datetime(2026, 11, 7, tzinfo=timezone.utc)
        with self.assertRaises(q.Refused):
            run(times=[NOW, expiry])
        with self.assertRaises(q.Refused):
            run(times=[NOW, NOW, expiry])
        earlier = datetime(2026, 11, 5, tzinfo=timezone.utc)
        with self.assertRaises(q.Refused):
            run(times=[NOW, earlier, NOW])
        with self.assertRaises(q.Refused):
            run(times=[NOW, NOW, earlier])

    def test_closure_must_be_complete_well_formed_and_safe(self):
        for closure in [{}, {".github": payload()["producer_closure"][".github"]},
                        {**payload()["producer_closure"], "../evil": {"kind": "blob", "sha": "f" * 40, "mode": "100644"}},
                        {**payload()["producer_closure"], "scripts/control.py": {"kind": "blob", "sha": "f" * 40, "mode": "120000"}}]:
            data = payload()
            data["producer_closure"] = closure
            with self.assertRaises(q.Refused):
                run([listing([check(data)]), listing([check(data)])])

    def test_changed_observation_or_current_head_between_reads_refuses(self):
        later = check()
        later["id"] = 72
        with self.assertRaises(q.Refused):
            run([listing(), listing([later])])
        later_current = current()
        later_current["head_sha"] = "1" * 40
        with self.assertRaises(q.Refused):
            run(currents=[json.dumps(current()), json.dumps(current()), json.dumps(later_current)])
        later_payload = deepcopy(payload())
        later_payload["authority_sha256"] = "0" * 64
        with self.assertRaises(q.Refused):
            run([listing(), listing([check(later_payload)])])


    def test_previous_entry_is_not_a_duplicate_current_acknowledgement(self):
        old = payload()
        old["entry_id"] = "entry-previous"
        old_check = check(old)
        old_check["id"] = 70
        result, _ = run([listing([old_check, check()]), listing([old_check, check()])])
        self.assertEqual(result["check_run_id"], 71)

    def test_control_characters_cannot_form_a_queue_entry_identity(self):
        binding = q.Binding(BINDING.repository, 42, H, "entry\x00id", BINDING.publisher_app_id,
                            BINDING.publisher_app_name, BINDING.publisher_check_name,
                            GEN, BINDING.expected_closure)
        with self.assertRaises(q.Refused):
            binding.validate()

    def test_current_pr_must_remain_in_same_repository_and_base(self):
        for key, value in [("head_repository", "someone/fork"), ("base_ref", "other")]:
            changed = current()
            changed[key] = value
            with self.subTest(key=key), self.assertRaises(q.Refused):
                run(currents=[json.dumps(current()), json.dumps(current()), json.dumps(changed)])

    def test_exact_trusted_closure_hash_and_mode_are_enforced(self):
        for key, value in [("sha", "0" * 40), ("mode", "100755")]:
            data = payload()
            data["producer_closure"]["scripts/control.py"][key] = value
            with self.subTest(key=key), self.assertRaises(q.Refused):
                run([listing([check(data)]), listing([check(data)])])

    def test_nested_duplicate_invalid_utf8_and_overbound_json_refuse(self):
        for raw in [b"\xff", b"{" + b" " * q.MAX_BYTES + b"}",
                    b'{"a":{"x":1,"x":1}}', b'{"a":1e999}',
                    '{"a":1}'.encode("utf-16")]:
            with self.subTest(prefix=raw[:12]), self.assertRaises(q.Refused):
                q.decode(raw)

    def test_trusted_closure_must_include_github_tree_and_independent_root(self):
        alternatives = [{".github": BINDING.expected_closure[".github"]},
                        {".github": BINDING.expected_closure[".github"],
                         ".github/actions/tool": BINDING.expected_closure["scripts/control.py"]},
                        {"scripts/control.py": BINDING.expected_closure["scripts/control.py"]},
                        {**BINDING.expected_closure,
                         ".github": {"kind": "blob", "sha": "e" * 40, "mode": "100644"}}]
        for expected in alternatives:
            binding = q.Binding(BINDING.repository, 42, H, "entry-42", BINDING.publisher_app_id,
                                BINDING.publisher_app_name, BINDING.publisher_check_name, GEN, expected)
            with self.subTest(expected=expected), self.assertRaises(q.Refused):
                run(binding=binding)

    def test_fabricated_self_declaration_never_changes_the_claim(self):
        fabricated = current()
        fabricated["authenticated"] = True
        result, _ = run(currents=[json.dumps(fabricated)] * 3)
        self.assertEqual(result["status"], "VALID_AGAINST_SUPPLIED_OBSERVATIONS")
        self.assertNotIn("authenticated", result)

    def test_refuses_noninteger_counts_and_oversized_check_sets(self):
        for response in [json.dumps({"total_count": True, "check_runs": []}),
                         json.dumps({"total_count": 101, "check_runs": [check()] * 101}),
                         json.dumps({"total_count": 1, "check_runs": None})]:
            with self.assertRaises(q.Refused):
                run([response, response])

    def test_intervening_observation_is_mandatory_and_binding_mutation_refuses(self):
        changed_binding = deepcopy(BINDING)
        events = []
        def mutate():
            events.append("between")
            changed_binding.expected_closure["scripts/control.py"]["sha"] = "0" * 40
        with self.assertRaises(q.Refused):
            q.observe(changed_binding, lambda _: listing(), lambda: json.dumps(current()),
                      lambda: NOW, mutate)
        self.assertEqual(events, ["between"])

    def test_completion_clock_cannot_change_the_trusted_binding(self):
        changed_binding = deepcopy(BINDING)
        count = 0
        def clock():
            nonlocal count
            count += 1
            if count == 3:
                changed_binding.expected_closure["scripts/control.py"]["sha"] = "0" * 40
            return NOW
        with self.assertRaises(q.Refused):
            q.observe(changed_binding, lambda _: listing(), lambda: json.dumps(current()),
                      clock, lambda: None)
        self.assertEqual(count, 3)

    def test_unconfigured_or_actions_app_cannot_become_authority(self):
        for app_id, name, check_name, generation, closure in [(0, "App", "Check", GEN, BINDING.expected_closure),
                (15368, "Actions", "Check", GEN, BINDING.expected_closure),
                (123, "", "Check", GEN, BINDING.expected_closure),
                (123, "App", "", GEN, BINDING.expected_closure),
                (123, "App", "Check", "", BINDING.expected_closure),
                (123, "App", "Check", GEN, {})]:
            binding = q.Binding(BINDING.repository, 42, H, "entry-42", app_id, name, check_name, generation, closure)
            with self.assertRaises(q.Refused):
                run(binding=binding)


if __name__ == "__main__":
    unittest.main()
