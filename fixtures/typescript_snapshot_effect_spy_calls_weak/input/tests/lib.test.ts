import { enqueue } from "../src/lib";

test("enqueue", () => {
    const spy = jest.spyOn(console, "log");
    enqueue([], 7);
    expect(spy.mock.calls).toMatchInlineSnapshot(`[]`);
});
