import { test, expect } from "vitest";
import { buildProfile } from "../src/profile";

test("profile records adulthood", () => {
  const bo = buildProfile("bo", 18);
  bo.adult = true;
  expect(bo).toMatchObject({ name: "bo", adult: true });
});
