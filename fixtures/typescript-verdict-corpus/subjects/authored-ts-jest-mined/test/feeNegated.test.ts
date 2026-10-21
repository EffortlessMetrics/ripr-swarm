import { feeCents } from "../src/feeNegated";

expect.extend({
  toBeCents(received: number, expected: number) {
    const pass = received === expected;
    return { pass, message: () => `expected ${received} to be exactly ${expected} cents` };
  },
});

declare global {
  namespace jest {
    interface Matchers<R> {
      toBeCents(expected: number): R;
    }
  }
}

test("fees are not off by one cent", () => {
  expect(feeCents(1.5)).not.toBeCents(151);
});
