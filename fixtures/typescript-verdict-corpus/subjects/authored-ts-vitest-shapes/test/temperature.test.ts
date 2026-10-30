import { expect, it } from "vitest";
import { Temperature } from "../src/temperature";

it("boiling point converts exactly", () => {
  expect(Temperature.fromFahrenheit(212).celsius).toBe(100);
});

it("freezing point converts exactly", () => {
  expect(Temperature.fromFahrenheit(32).celsius).toBe(0);
});
