//! Entry forms recorded for a test file that replaces a function on its
//! module object (#15939). The relation and rendered-finding proofs run the
//! built binary in `crates/ripr/tests/typescript_replaced_owner_function.rs`.

use super::super::function_replacement::replaced_function_member_names;

#[test]
fn replacement_names_record_the_documented_entry_forms() {
    let names = |source: &str| replaced_function_member_names(source);
    assert_eq!(names("sinon.stub(ns, 'run');"), ["ns.run"]);
    assert_eq!(names("sinon.stub(ns.default, 'run');"), ["ns.run"]);
    assert_eq!(names("sinon.stub(load(), 'run');"), ["run"]);
    assert_eq!(names("sinon.stub(ns);"), ["ns.*"]);
    assert_eq!(names("sinon.stub(load(), key);"), ["*"]);
    assert_eq!(names("sinon.stub((ns as any), key);"), ["ns.*"]);
    assert_eq!(names("ns.run = fake;"), ["ns.run"]);
    assert_eq!(names("(ns as any).run = fake;"), ["ns.run"]);
    assert_eq!(names("a.b.run ??= fake;"), ["a.run"]);
    assert_eq!(names("load().run = fake;"), ["run"]);
    assert_eq!(names("rows[0].run = fake;"), ["run"]);
    assert_eq!(names("t.mock.method(ns, 'run', fake);"), ["ns.run"]);
    assert_eq!(
        names("mock.method(ns, 'run'); ns.run.mock.mockImplementation(fake);"),
        ["ns.run"]
    );
    assert_eq!(
        names("const s = jest.spyOn(ns, 'run');\ns.mockResolvedValueOnce(1);"),
        ["ns.run"]
    );
    let nothing: [&str; 0] = [];
    assert_eq!(names("sinon.stub();"), nothing);
    assert_eq!(names("text.replace('run', 'walk');"), nothing);
    assert_eq!(names("mock.method(ns, 'run');"), nothing);
    assert_eq!(names("jest.spyOn(ns, 'run');"), nothing);
    assert_eq!(names("sinon.spy(ns, 'run');"), nothing);
    assert_eq!(names("if (ns.run === fake) {}"), nothing);
    assert_eq!(
        names("const f = (x) => ns.run; expect(ns.run == f);"),
        nothing
    );
    assert_eq!(names("restub(ns, 'run'); stubbed(ns, 'run');"), nothing);
}
