import { Form } from "../src/minimum";

test("a valid quantity is stored", () => {
  const form = new Form();
  form.quantity = 7;
  expect(form.quantity).toBe(7);
});

test("a quantity below the limit is refused", () => {
  const form = new Form();
  expect(() => {
    form.quantity = 2;
  }).toThrow("quantity below 3");
});
