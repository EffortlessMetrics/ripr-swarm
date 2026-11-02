import { Outbox } from "../src/lib";

test("send", () => {
    const box = new Outbox();
    expect(box.send(7)).toMatchInlineSnapshot(`
[
  "shipped 7",
]
`);
});
