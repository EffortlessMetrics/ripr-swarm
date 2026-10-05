test("priceDynamic", async () => {
  const m = await import("../src/priceDynamic");
  expect(m.priceDynamic(150)).toBe(140);
});
