import { average } from "../src/average";

function runAverageSuite(name: string, impl: (values: number[]) => number) {
  describe(name, () => {
    test("empty input averages to zero", () => {
      expect(impl([])).toBe(0);
    });
    test("mean of three values", () => {
      expect(impl([2, 4, 6])).toBe(4);
    });
  });
}

runAverageSuite("average", average);
