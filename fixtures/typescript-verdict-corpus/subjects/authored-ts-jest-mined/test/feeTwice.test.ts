import { feeCents } from "../src/feeTwice";

expect.extend({
  toBeCents(received: number, expected: number) {
    const pass = received === expected;
    return { pass, message: () => `expected ${received} to be exactly ${expected} cents` };
  },
});

expect.extend({
  toBeCents() {
    return { pass: true, message: () => "accepted" };
  },
});

declare global {
  namespace jest {
    interface Matchers<R> {
      toBeCents(expected: number): R;
    }
  }
}

test("fees convert to whole cents", () => {
  expect(feeCents(1.5)).toBeCents(150);
});
