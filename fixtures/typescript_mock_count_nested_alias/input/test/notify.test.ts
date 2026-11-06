import { notify } from "../src/notify.ts";
it("notify nested_alias", done => {
  const fn = jest.fn();
  notify(4, {send: fn});
  setTimeout(() => {
    const calls = fn.mock.calls; expect(calls.length).toBe(1);
    done();
  }, 0);
});
