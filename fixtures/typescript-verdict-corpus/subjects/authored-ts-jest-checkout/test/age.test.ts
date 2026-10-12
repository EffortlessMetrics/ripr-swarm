import { isAdult } from "../src/age";

test("a thirty year old is not refused", () => {
  expect(isAdult(30)).not.toBe(false);
});
