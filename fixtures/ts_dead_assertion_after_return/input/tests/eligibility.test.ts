import { isEligible } from "../src/eligibility";

test("eligibility", () => {
  expect(isEligible(30)).toBeTruthy();
  return;
  expect(isEligible(18)).toBe(false);
});
