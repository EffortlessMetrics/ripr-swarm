<!-- section: Internal / CI -->
- A real stdio journey pins the landed #13280 gap-list not-analyzed contract end to end: a default-base empty refresh lists `not_analyzed` on the same snapshot, an explicit parent base yields the real finding (with filter-to-zero and past-end controls staying silent), and a wrong-base refresh fails closed without disturbing the retained last-good listing (#13280).
