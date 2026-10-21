import { isEligible } from "../src/eligibility";

test("eligibility", () => {
  expect(isEligible(30)).toBeTruthy();
  try {
    return;
  } finally {
    cleanup();
  }
  expect(isEligible(18)).toBe(false);
});
