<!-- section: Fixed -->
- Python bare-bool assert credit no longer fails closed on unrelated `mock.patch` / `monkeypatch.setattr` routes in the same pytest module when those calls do not name the bool owner; callee-targeted patch routes and unreadable targets stay conservative (#10391).
