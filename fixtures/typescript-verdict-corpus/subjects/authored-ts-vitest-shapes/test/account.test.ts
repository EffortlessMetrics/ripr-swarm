import { expect, it } from "vitest";
import { Account } from "../src/account";

it("deposit returns the new balance", () => {
  expect(new Account(10).deposit(5)).toBe(15);
});

it("a negative balance is overdrawn", () => {
  expect(new Account(-5).overdrawn).toBe(true);
});

it("a positive balance is not overdrawn", () => {
  expect(new Account(5).overdrawn).toBe(false);
});
