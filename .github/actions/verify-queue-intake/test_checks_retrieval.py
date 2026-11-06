"""Injected provider fixtures; no live GitHub or credentials are used."""

from copy import deepcopy
from dataclasses import replace
from datetime import timedelta
import json
import unittest
from urllib.parse import parse_qs, urlsplit

import checks_retrieval as adapter
import queue_intake as q
from test_queue_intake import BINDING, H, NOW, check, current, payload


ACK = adapter.EnqueueAcknowledgement(BINDING.repository, 42, H, "entry-42",
                                     "admission-1", "enqueue-1")


def page(rows, total=None, next_page=None):
    return adapter.Page(200, json.dumps({"total_count": len(rows) if total is None else total,
                                         "check_runs": rows}).encode(), next_page)


class Transport:
    def __init__(self, responses):
        self.responses = iter(responses)
        self.requests = []

    def get(self, repository, path):
        self.requests.append((repository, path))
        value = next(self.responses)
        if isinstance(value, Exception):
            raise value
        return value


def run(responses=None, ack=ACK, binding=BINDING, currents=None, times=None, between=lambda: None):
    transport = Transport(responses if responses is not None else [page([check()])] * 2)
    current_values = iter(currents if currents is not None else [json.dumps(current())] * 3)
    time_values = iter(times if times is not None else [NOW] * 3)
    result = adapter.observe(binding, ack, transport, lambda: next(current_values),
                             lambda: next(time_values), between)
    return result, transport.requests


class ChecksRetrievalTests(unittest.TestCase):
    def test_complete_exact_h_read_twice_and_no_authority(self):
        other = check()
        other["id"] = 72
        other["app"] = {"id": 99, "name": "Other"}
        responses = [page([other], 2, 2), page([check()], 2)] * 2
        result, requests = run(responses)
        self.assertEqual(result["check_run_id"], 71)
        self.assertEqual(result["authorization"], "NotEstablished")
        self.assertEqual(result["qualification"], "NotEstablished")
        self.assertEqual(result["intake"]["base_sha"], "c" * 40)
        self.assertEqual(len(requests), 4)
        for index, (repo, path) in enumerate(requests):
            parsed = urlsplit(path)
            self.assertEqual(repo, BINDING.repository)
            self.assertEqual(parsed.path, f"/commits/{H}/check-runs")
            self.assertEqual(parse_qs(parsed.query), {"check_name": [BINDING.publisher_check_name],
                             "filter": ["all"], "per_page": ["100"],
                             "page": [str(index % 2 + 1)]})

    def test_lost_or_wrong_acknowledgement_refuses_before_network(self):
        for ack in [None, replace(ACK, repository="elsewhere/repo"), replace(ACK, pull_number=43),
                    replace(ACK, head_sha="b" * 40), replace(ACK, entry_id="other"),
                    replace(ACK, admission_request_id=""), replace(ACK, enqueue_request_id="")]:
            with self.subTest(ack=ack):
                transport = Transport([])
                with self.assertRaises(q.Refused):
                    adapter.observe(BINDING, ack, transport, lambda: json.dumps(current()),
                                    lambda: NOW, lambda: None)
                self.assertEqual(transport.requests, [])

    def test_receipt_request_ids_must_match_intake(self):
        for ack in [replace(ACK, admission_request_id="another"),
                    replace(ACK, enqueue_request_id="another")]:
            with self.subTest(ack=ack), self.assertRaises(q.Refused):
                run(ack=ack)

    def test_bad_pages_are_refused_without_retry(self):
        wrong = check()
        wrong["id"] = 72
        cases = [[page([check()], 2)] * 2, [page([check()], 2, 2), page([check()], 2)],
                 [page([check()], 2, 2), page([wrong], 3)], [page([], 2, 2)],
                 [page([check()], 1, 2)], [page([check()], 2, 3)],
                 [page([check()], 101)], [adapter.Page(403, b"{}", None)],
                 [adapter.Page(200, b'{"total_count":1,"total_count":1,"check_runs":[]}', None)],
                 [adapter.Page(200, b'{"total_count":NaN,"check_runs":[]}', None)],
                 [adapter.Page(200, b'{"total_count":0,"check_runs":[],"x":1e999}', None)],
                 [adapter.Page(200, b'{"x":' + b'[' * 2000 + b'0' + b']' * 2000 + b'}', None)],
                 [adapter.Page(200, b" " * (q.MAX_BYTES + 1), None)],
                 [OSError("transport failure")]]
        for responses in cases:
            with self.subTest(responses=responses):
                transport = Transport(responses)
                with self.assertRaises(q.Refused):
                    adapter.observe(BINDING, ACK, transport, lambda: json.dumps(current()),
                                    lambda: NOW, lambda: None)
                self.assertLessEqual(len(transport.requests), len(responses))

    def test_provider_error_is_not_retried(self):
        transport = Transport([adapter.Page(403, b"{}", None), page([check()])])
        with self.assertRaises(q.Refused):
            adapter.observe(BINDING, ACK, transport, lambda: json.dumps(current()),
                            lambda: NOW, lambda: None)
        self.assertEqual(len(transport.requests), 1)

    def test_two_valid_page_sets_cannot_hide_duplicate_producers(self):
        duplicate = {**check(), "id": 72}
        pages = [page([check()], 2, 2), page([duplicate], 2)] * 2
        with self.assertRaises(q.Refused):
            run(pages)

    def test_app_head_external_id_and_conclusion_are_bound(self):
        changes = [("app", {"id": 15368, "name": BINDING.publisher_app_name}),
                   ("app", {"id": BINDING.publisher_app_id, "name": "wrong"}),
                   ("name", "other"), ("head_sha", "f" * 40),
                   ("external_id", "hermes-queue:other:H"), ("status", "in_progress"),
                   ("conclusion", "failure"), ("id", True)]
        for key, value in changes:
            bad = check()
            bad[key] = value
            with self.subTest(key=key), self.assertRaises(q.Refused):
                run([page([bad])] * 2)
        with self.assertRaises(q.Refused):
            run([page([check(), {**check(), "id": 72}])] * 2)

    def test_strict_intake_and_nested_json_controls(self):
        cases = []
        for key, value in [("schema_version", 1), ("pull_number", True),
                           ("repository", "other/repo"), ("entry_id", "other"),
                           ("producer_generation_sha256", "0" * 64),
                           ("expires_at", "2026-11-05T00:00:00Z")]:
            data = payload()
            data[key] = value
            cases.append(json.dumps(data))
        data = payload()
        data["producer_closure"]["scripts/control.py"]["sha"] = "0" * 40
        cases.append(json.dumps(data))
        cases.extend(['{"entry_id":"entry-42","entry_id":"entry-42"}',
                      '{"entry_id":"entry-42","x":NaN}',
                      '{"entry_id":"entry-42","x":' + '[' * 2000 + '0' + ']' * 2000 + '}'])
        for raw in cases:
            bad = check()
            bad["output"]["text"] = raw
            with self.subTest(raw=raw[:60]), self.assertRaises(q.Refused):
                run([page([bad])] * 2)

    def test_changed_entry_check_and_expiry_during_reads_refuse(self):
        changed = current()
        changed["entry_id"] = "other"
        with self.assertRaises(q.Refused):
            run(currents=[json.dumps(current())] * 2 + [json.dumps(changed)])
        changed_check = check()
        changed_check["id"] = 72
        with self.assertRaises(q.Refused):
            run([page([check()]), page([changed_check])])
        other = {**check(), "id": 72, "app": {"id": 99, "name": "Other"}}
        changed_other = {**other, "id": 73}
        with self.assertRaises(q.Refused):
            run([page([check(), other]), page([check(), changed_other])])
        with self.assertRaises(q.Refused):
            run(times=[NOW, NOW, NOW + timedelta(days=2)])

    def test_result_order_changes_do_not_change_the_observed_set(self):
        other = {**check(), "id": 72, "app": {"id": 99, "name": "Other"}}
        result, _ = run([page([check(), other]), page([other, check()])])
        self.assertEqual(result["check_run_id"], 71)

    def test_configuration_mutation_and_unconfigured_publisher_refuse(self):
        binding = deepcopy(BINDING)
        def mutate():
            binding.expected_closure["scripts/control.py"]["sha"] = "0" * 40
        with self.assertRaises(q.Refused):
            run(binding=binding, between=mutate)
        for binding in [replace(BINDING, publisher_app_id=15368),
                        replace(BINDING, publisher_app_id=0),
                        replace(BINDING, publisher_app_name="")]:
            with self.subTest(binding=binding), self.assertRaises(q.Refused):
                run(binding=binding)


if __name__ == "__main__":
    unittest.main()
