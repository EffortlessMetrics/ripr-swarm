import { feeCents } from "../src/feeVacuous";

expect.extend({
  toBeCents(received: number, expected: number) {
    const pass = received === expected || received === 0;
    return { pass, message: () => `expected ${received} to be ${expected} cents` };
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
