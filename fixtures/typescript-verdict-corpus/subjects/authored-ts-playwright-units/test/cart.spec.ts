import { expect, test as base } from "@playwright/test";
import { Cart } from "../src/cart";

const test = base.extend<{ cart: Cart }>({
  cart: async ({}, use) => {
    await use(new Cart());
  },
});

test("an empty cart totals zero", ({ cart }) => {
  expect(cart.total()).toBe(0);
});

test("lines add up", ({ cart }) => {
  cart.add(250);
  cart.add(150);
  expect(cart.total()).toBe(400);
});
