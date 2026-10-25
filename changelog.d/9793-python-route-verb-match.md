<!-- section: Fixed -->
- Python route handlers no longer take test credit from a client call with a different HTTP verb than the decorator (`client.get` against `@app.post`); `methods=[...]` lists are honored and an unknown verb keeps the path match (#9793).
