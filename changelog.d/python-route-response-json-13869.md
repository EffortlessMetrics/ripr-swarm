<!-- section: Fixed -->
- Python: a FastAPI-style `TestClient` test over the owner's own app that
  pins the changed key of the handler's returned dict, with
  `assert response.json()["total"] == 190` or
  `self.assertEqual(response.json()["total"], 190)`, now reads `exposed`.
  Before, ripr reported a gap even though the test already checked the
  changed value. A `status_code` check, another key, another route or an
  unidentified app still never reads `exposed`, and neither does a handler
  with a response model, a return annotation, another decorator, another
  `return` or a `raise`, or a changed `detail` key (#13869).
