import { parseEx13 } from "../src/parseEx13";

test("parseEx13", () => {
  expect(() => parseEx13("")).toThrow(Error);
});
