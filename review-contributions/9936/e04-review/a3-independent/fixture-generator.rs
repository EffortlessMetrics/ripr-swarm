fn tape_test(prelude: &str, tail: &str) -> String {
    format!(
        "{prelude}\nconst {{ triple }} = require('../src/k.js');\n\ntest('triples', function (t) {{\n  t.equal(triple(2), 6);\n  t.end();\n}});\n{tail}"
    )
}
fn main() {
    let regex = "const q = n</a`/.source.length; test = null;";
    let tsx_regex = "const q = n < /a`/.source.length; test = null;";
    let cases = [
        (
            "test/k.test.ts",
            format!("const n = 1, s = ''; const a = <any>s; {regex}"),
        ),
        (
            "test/k.test.ts",
            format!("const n = 1, s = ''; 1<<n; {regex}"),
        ),
        (
            "test/k.test.tsx",
            format!("const n = 1, s = ''; const a = <Foo a={{1}} />; {tsx_regex}"),
        ),
        (
            "test/k.test.tsx",
            format!(
                "const n = 1, s = ''; function f() {{ return <p>x</p>; test = null; }} {tsx_regex}"
            ),
        ),
        (
            "test/k.test.ts",
            "async function f() { const s = ''; for await (const x of []) /a`/.test(s); test = null; }"
                .to_string(),
        ),
    ];
    for (index, (file, tail)) in cases.iter().enumerate() {
        let ext = if file.ends_with("tsx") { "tsx" } else { "ts" };
        let dir = "/workspace/9936-a3-review";
        std::fs::write(format!("{dir}/tail-{index}.{ext}"), tail).unwrap();
        let fixture = tape_test("var test = require('tape');", tail);
        std::fs::write(format!("{dir}/fixture-{index}.{ext}"), &fixture).unwrap();
        let declarations = "export {};\ndeclare function require(path: string): any;\ndeclare function Foo(props: any): any;\ndeclare global { namespace JSX { type Element=any; interface IntrinsicElements { p: any; } } }\n";
        std::fs::write(format!("{dir}/native-{index}.{ext}"), format!("{declarations}{fixture}")).unwrap();
    }
}
