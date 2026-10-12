import { parseEx12 } from "../src/parseEx12";

test("parseEx12", () => {
  expect(() => parseEx12("")).toThrow("empty");
});
