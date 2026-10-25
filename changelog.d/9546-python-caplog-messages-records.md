<!-- section: Changed -->
- Python: `caplog.messages` and `any(...)` over `caplog.records` assertions on a changed logging call are evaluated like `caplog.text` (a whole formatted message or a substring of one), so an edit that removes the asserted message is credited instead of reading a weak gap; indexed, filtered and whole-list forms abstain (#9546).
