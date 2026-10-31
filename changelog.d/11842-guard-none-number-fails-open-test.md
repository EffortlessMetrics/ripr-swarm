<!-- section: Internal / CI -->
- The Python guard-activation unit test now expects `None != 0` to fail open (`None`) after the shared equality helper stopped equating mixed kinds (#11842); it was left asserting the old definite answer and failed on main.
