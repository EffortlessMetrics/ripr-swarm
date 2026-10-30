<!-- section: Changed -->
- Python: a `<text> in caplog.text` assertion on a changed logging call is evaluated when the test pins the capture level and the message is `%`-formatted from literal inputs, so an edit that removes the asserted text or drops the call below the pinned level is credited instead of abstaining; every other shape still abstains (#9546).
