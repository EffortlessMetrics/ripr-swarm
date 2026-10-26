//! Generated workspaces: one large file in a language, role and shape, plus
//! a small changed file whose diff is what `ripr check` analyzes.

pub(crate) const DIFF: &str = "change.diff";
pub(crate) const LANGUAGES: [&str; 3] = ["rust", "python", "typescript"];

#[derive(Clone, Copy)]
pub(crate) struct Cell {
    pub(crate) language: &'static str,
    pub(crate) role: &'static str,
    pub(crate) shape: &'static str,
}

impl Cell {
    pub(crate) fn id(&self) -> String {
        format!("{}-{}-{}", self.language, self.role, self.shape)
    }

    /// The small file the diff changes; `ripr check` must report a finding
    /// for it, or the row measured nothing.
    pub(crate) fn changed_path(&self) -> &'static str {
        match self.language {
            "rust" => "src/lib.rs",
            "python" => "src/pkg/small.py",
            _ => "src/small.ts",
        }
    }

    /// The large unchanged file whose cost is being measured.
    pub(crate) fn big_path(&self) -> &'static str {
        match (self.language, self.role) {
            ("rust", "production") => "src/big.rs",
            ("rust", _) => "tests/big.rs",
            ("python", "production") => "src/pkg/big.py",
            ("python", _) => "tests/test_big.py",
            (_, "production") => "src/big.ts",
            _ => "src/big.test.ts",
        }
    }

    /// Workspace files (relative path, bytes) with the large file at about
    /// `bytes` bytes.
    pub(crate) fn files(&self, bytes: usize) -> Vec<(String, Vec<u8>)> {
        let multi = self.shape.starts_with("lines");
        let big = fill(bytes, |index| unit(self, multi, index));
        let mut files = match self.language {
            "rust" => rust_files(self.role, big),
            "python" => python_files(self.role, big),
            _ => typescript_files(self.role, big),
        };
        files.push((DIFF.to_string(), diff(self.language).into_bytes()));
        files
    }
}

pub(crate) fn cells(languages: &[String]) -> Vec<Cell> {
    let mut out = Vec::new();
    for language in LANGUAGES {
        if !languages.is_empty() && !languages.iter().any(|name| name == language) {
            continue;
        }
        for role in ["production", "test"] {
            for shape in ["one-line", "lines"] {
                // A Python test is a `def`, which cannot share a line with the
                // next one, so there is no one-line test file to generate.
                if language == "python" && role == "test" && shape == "one-line" {
                    continue;
                }
                out.push(Cell {
                    language,
                    role,
                    shape,
                });
            }
        }
    }
    out
}

fn fill(bytes: usize, unit: impl Fn(usize) -> String) -> String {
    let mut text = String::with_capacity(bytes + 128);
    let mut index = 0;
    while text.len() < bytes {
        text.push_str(&unit(index));
        index += 1;
    }
    text
}

/// One repeated item. `lines` puts each item on its own line; `one-line` runs
/// them together so a single line holds the whole file.
fn unit(cell: &Cell, multi: bool, index: usize) -> String {
    let (item, tail) = match (cell.language, cell.role) {
        ("rust", "production") => (format!("pub fn f{index}() -> u32 {{ {index} }}"), " "),
        ("rust", _) => (
            format!("#[test] fn t{index}() {{ assert_eq!({index}, {index}); }}"),
            " ",
        ),
        ("python", "production") => (format!("a{index} = {index}"), ";"),
        ("python", _) => (
            format!("def test_{index}():\n    assert {index} == {index}"),
            "",
        ),
        (_, "production") => (
            format!("export function f{index}() {{ return {index}; }}"),
            " ",
        ),
        _ => (
            format!(
                "let v{index} = {index}; v{index} = v{index} + 1; test('t{index}', () => {{ expect(v{index}).toBe({}); }});",
                index + 1
            ),
            " ",
        ),
    };
    if multi {
        format!("{item}\n")
    } else {
        format!("{item}{tail}")
    }
}

fn rust_files(role: &str, big: String) -> Vec<(String, Vec<u8>)> {
    let mut lib = "pub fn small(x: u32) -> u32 { x + 2 }\n".to_string();
    let mut files = Vec::new();
    if role == "production" {
        lib.push_str("pub mod big;\n");
        files.push(("src/big.rs".to_string(), big.into_bytes()));
    } else {
        files.push(("tests/big.rs".to_string(), big.into_bytes()));
    }
    files.push(("src/lib.rs".to_string(), lib.into_bytes()));
    files.push((
        "Cargo.toml".to_string(),
        b"[package]\nname = \"size_probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n".to_vec(),
    ));
    files
}

fn python_files(role: &str, big: String) -> Vec<(String, Vec<u8>)> {
    let path = if role == "production" {
        "src/pkg/big.py"
    } else {
        "tests/test_big.py"
    };
    vec![
        (path.to_string(), big.into_bytes()),
        (
            "src/pkg/small.py".to_string(),
            b"def small(x): return x + 2\n".to_vec(),
        ),
        ("src/pkg/__init__.py".to_string(), Vec::new()),
        (
            "pyproject.toml".to_string(),
            b"[project]\nname = \"size_probe\"\nversion = \"0.1\"\n".to_vec(),
        ),
    ]
}

fn typescript_files(role: &str, big: String) -> Vec<(String, Vec<u8>)> {
    let path = if role == "production" {
        "src/big.ts"
    } else {
        "src/big.test.ts"
    };
    vec![
        (path.to_string(), big.into_bytes()),
        (
            "src/small.ts".to_string(),
            b"export function small(a: number) { return a + 2; }\n".to_vec(),
        ),
        (
            "package.json".to_string(),
            b"{\"name\":\"size_probe\"}\n".to_vec(),
        ),
    ]
}

fn diff(language: &str) -> String {
    let (path, before, after) = match language {
        "rust" => (
            "src/lib.rs",
            "pub fn small(x: u32) -> u32 { x + 1 }",
            "pub fn small(x: u32) -> u32 { x + 2 }",
        ),
        "python" => (
            "src/pkg/small.py",
            "def small(x): return x + 1",
            "def small(x): return x + 2",
        ),
        _ => (
            "src/small.ts",
            "export function small(a: number) { return a + 1; }",
            "export function small(a: number) { return a + 2; }",
        ),
    };
    format!(
        "diff --git a/{path} b/{path}\nindex 0000000..1111111 100644\n--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-{before}\n+{after}\n"
    )
}
