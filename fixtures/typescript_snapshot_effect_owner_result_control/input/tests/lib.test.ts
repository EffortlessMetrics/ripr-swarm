import { notify } from "../src/lib";

test("notify", () => {
    expect(notify(7)).toMatchInlineSnapshot(`undefined`);
});
