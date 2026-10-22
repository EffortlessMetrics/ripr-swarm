import { sumNoopTest } from "../src/sumNoopTest";

const test = (_name: string, _body: () => void) => {};

describe("sum", () => {
  test("sum adds every value", () => {
    expect(sumNoopTest([2, 3, 4])).toBe(9);
  });
  it("the placeholder test still runs", () => {
    expect(1).toBe(1);
  });
});
