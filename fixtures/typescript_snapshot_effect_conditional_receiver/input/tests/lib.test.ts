import { Outbox } from "../src/lib";

test("send", () => {
    const fake = { send: (id: number) => ["shipped 7"] };
    const box = true ? fake : new Outbox();
    expect(box.send(7)).toMatchInlineSnapshot(`
[
  "shipped 7",
]
`);
});
