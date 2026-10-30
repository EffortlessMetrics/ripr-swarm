import { ready } from "../src/identityExtend";

expect.extend({
  toBeSame(received: unknown, expected: unknown) {
    return { pass: received === expected, message: () => "expected the same object" };
  },
});

declare global {
  namespace jest {
    interface Matchers<R> {
      toBeSame(expected: unknown): R;
    }
  }
}

test("ready hands back the same config", () => {
  const cfg = { name: "bo", ok: true };
  expect(ready(cfg)).toBeSame(cfg);
});
