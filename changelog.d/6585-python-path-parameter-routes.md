<!-- section: Fixed -->
- Python preview now relates a route with a path parameter, such as
  `@app.get("/items/{id}")` or `/items/<int:id>`, to a test that calls a
  concrete path like `client.get("/items/1")`, instead of reporting
  `no_static_path` (#6585).
