<!-- section: Fixed -->
- TypeScript and JavaScript: a class method the test file replaces (`c.apply = fake`, `vi.spyOn(Cart.prototype, 'apply')`, a sinon `stub`, node `mock.method`) or calls on a receiver the test rebinds (`c = new Other()`) no longer keeps a direct owner-call relation. The same holds for a replaced static method (#15950, #6980).
