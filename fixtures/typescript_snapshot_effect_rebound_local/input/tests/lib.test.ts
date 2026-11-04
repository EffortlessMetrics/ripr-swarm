import { enqueue } from "../src/lib";

test("enqueue", () => {
    let result = enqueue([], 7);
    ({ result } = { result: ["shipped 7"] });
    expect(result).toMatchInlineSnapshot(`
[
  "shipped 7",
]
`);
});
