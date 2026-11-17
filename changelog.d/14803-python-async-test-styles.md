<!-- section: Fixed -->
- Python async tests read like their sync forms: a test in a
  `unittest.IsolatedAsyncioTestCase` class no longer reads `no_static_path`,
  and a strong assertion is credited through an `asyncSetUp` receiver, a
  `result = await owner(..)` local, or a `@pytest_asyncio.fixture` coroutine
  fixture that returns a literal-built owner object. A plain `pytest.fixture`
  on a coroutine def stays unread (#14803).
