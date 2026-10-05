import { buildReceipt } from "../src/receipt";

test("receipt shape", () => {
  expect(buildReceipt(7, 42)).toMatchInlineSnapshot(`
{
  "currency": "USD",
  "id": 7,
  "total": 42,
}
`);
});
