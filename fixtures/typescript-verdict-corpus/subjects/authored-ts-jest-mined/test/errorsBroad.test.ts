import { errorForBroad } from "../src/errorsBroad";

test("status codes map to some error", () => {
  expect(errorForBroad(404)).toBeInstanceOf(Error);
  expect(errorForBroad(403)).toBeInstanceOf(Error);
});
