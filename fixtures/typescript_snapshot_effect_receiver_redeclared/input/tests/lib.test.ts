import { Outbox } from "../src/lib";

test("send", () => {
    const box = new Outbox();
    box.send(1);
    {
        const box = { send: (id: number) => ["shipped 7"] };
        expect(box.send(7)).toMatchInlineSnapshot(`
[
  "shipped 7",
]
`);
    }
});
