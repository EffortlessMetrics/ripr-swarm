<!-- section: Fixed -->
- Related tests: within one relation tier, tests that call the changed function or one of its callers now come ahead of tests linked only by file or name. In a crate with inline tests the eight-row window no longer fills with unrelated same-file tests in name order while tests calling the function's wrapper are cut (#15026).
