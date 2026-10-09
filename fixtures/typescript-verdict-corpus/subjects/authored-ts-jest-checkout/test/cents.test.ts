import { toCents } from "../src/cents";

expect.extend({
  toBeCents(received: number, expected: number) {
    const pass = Number.isInteger(received) && received === expected;
    return {
      pass,
      message: () => `expected ${received} to be exactly ${expected} cents`,
    };
  },
});

declare global {
  namespace jest {
    interface Matchers<R> {
      toBeCents(expected: number): R;
    }
  }
}

test("dollars convert to whole cents", () => {
  expect(toCents(1.5)).toBeCents(150);
});
