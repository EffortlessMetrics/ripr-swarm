<!-- section: Fixed -->
- TypeScript mock-payload credit no longer accepts a mock binding that is
  not exactly a fresh mock: `jest.fn() && pre`, a named or `function`
  implementation, `{ write: jest.fn() }.inner`, or a next line that
  continues the expression now keep the medium reading. Arrow
  implementations, `vi.fn<T>()` and a statement that ends at a line break
  still credit. The `toMatchObject` field pin no longer reads a
  `__proto__:` entry as a listed field on either side
  (RIPR-SPEC-0247, RIPR-SPEC-0250, #8133, #8138).
