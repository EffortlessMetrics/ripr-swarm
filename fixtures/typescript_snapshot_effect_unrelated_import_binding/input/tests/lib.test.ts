import { enqueue as real } from "../src/lib";
import { enqueue } from "../src/other";

test("enqueue", () => {
    real([], 7);
    expect(enqueue([], 7)).toMatchInlineSnapshot(`
[
  "shipped 7",
]
`);
});
