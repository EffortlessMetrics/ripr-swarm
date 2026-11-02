import { enqueue } from "../src/lib";

test("enqueue", () => {
    expect(enqueue([], 7)).toMatchInlineSnapshot(`
[
  "shipped 7",
]
`);
});
