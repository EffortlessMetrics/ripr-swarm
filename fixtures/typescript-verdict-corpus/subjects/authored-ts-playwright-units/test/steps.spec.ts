import { expect, test } from "@playwright/test";
import { shipping } from "../src/steps";

test("quotes light and heavy parcels", async () => {
  await test.step("light parcel", async () => {
    expect(shipping(1)).toBe(400);
  });
  await test.step("heavy parcel", async () => {
    expect(shipping(20)).toBe(900);
  });
});
