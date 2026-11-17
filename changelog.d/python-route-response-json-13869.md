<!-- section: Fixed -->
- Python: a FastAPI-style `TestClient` test over the owner's own app that
  pins the changed key of the handler's returned dict against a literal,
  with `assert response.json()["total"] == 190` or
  `self.assertEqual(response.json()["total"], 190)`, now reads `exposed`.
  Before, ripr reported a gap even though the test already checked the
  changed value. The credit stays off whenever the response might not be
  the changed dict or the check might pass either way: a `status_code`
  check, another key or route, an unidentified app, an expected value that
  is not a literal, a response model or a return annotation other than a
  bare `dict`, another decorator, another `return`, a `raise`, a `**`
  unpack or repeated key, a changed `detail` key, a change JSON cannot see
  (tuple to list, `5` to `5.0`, `1` to `True`, a quote-style swap,
  `3.3 // 0.3` for `10`, `[10][0]` for `10`, a removed duplicate key), a
  response the test rewrites or rebinds, a `-> dict` handler in a workspace
  that may redefine `dict` (in any binding form, which withdraws only
  `-> dict` credit), application options passed through a `**` unpack,
  FastAPI's own docs routes, a route another route may share, and any workspace with exception handlers,
  middleware, router inclusions or route registrations ripr does not record,
  such as a directly applied or aliased route call (#13869).
