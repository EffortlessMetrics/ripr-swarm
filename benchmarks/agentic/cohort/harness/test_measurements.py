import unittest

from measurements import TOKEN_FIELDS, completed, terminal_usage, token_count


def result(**changes):
    row = dict(type="result", subtype="success", is_error=False, total_cost_usd=0.25,
               usage=dict(zip(TOKEN_FIELDS, (10, 20, 30, 40))))
    row.update(changes)
    return row


class MeasurementTests(unittest.TestCase):
    def test_cumulative_usage_preserves_cache_components(self):
        measured = terminal_usage(result())
        self.assertEqual(measured["tokens"], 100)
        self.assertEqual(measured["usage"]["cache_read_input_tokens"], 40)
        self.assertEqual(measured["usage_status"], "complete")
        self.assertEqual(measured["cost_source"], "sdk_estimate")
        self.assertEqual(token_count(measured), 100)

    def test_missing_or_partial_usage_is_not_zero(self):
        for final in ({}, result(usage=None), result(usage={"input_tokens": 10}),
                      result(type="assistant")):
            with self.subTest(final=final):
                self.assertIsNone(terminal_usage(final)["tokens"])

    def test_each_bad_component_invalidates_total_not_other_components(self):
        for key in TOKEN_FIELDS:
            for value in (None, True, -1, 1.5, "12", float("nan"), float("inf")):
                final = result()
                final["usage"][key] = value
                with self.subTest(key=key, value=value):
                    self.assertIsNone(terminal_usage(final)["tokens"])

    def test_explicit_zero_cache_counters_are_complete(self):
        final = result(usage=dict(zip(TOKEN_FIELDS, (10, 20, 0, 0))))
        self.assertEqual(terminal_usage(final)["tokens"], 30)

    def test_error_result_keeps_incurred_cost_without_completion(self):
        final = result(is_error=True, subtype="error_max_turns")
        measured = terminal_usage(final)
        self.assertEqual(measured["tokens"], 100)
        self.assertEqual(measured["cost_usd"], 0.25)
        self.assertFalse(completed(dict(measured, rc=0, is_error=True)))

    def test_failed_result_subtype_cannot_look_completed(self):
        self.assertFalse(completed(dict(rc=0, is_error=False, result_subtype="error_max_turns")))

    def test_partial_components_never_fall_back_to_reported_total(self):
        measured = terminal_usage(result(usage={"input_tokens": 100}))
        measured["tokens"] = 100
        self.assertIsNone(token_count(measured))

    def test_billing_estimate_rejects_invalid_numbers(self):
        for value in (True, -1, "0.1", float("nan"), float("inf")):
            with self.subTest(value=value):
                self.assertIsNone(terminal_usage(result(total_cost_usd=value))["cost_usd"])


if __name__ == "__main__":
    unittest.main()
