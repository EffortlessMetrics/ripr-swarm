use super::*;
use crate::analysis::classifier::classify_probe;
use crate::domain::{DeltaKind, Probe, ProbeFamily, ProbeId, SourceLocation};

fn subject(assertion: &str, method_body: Option<&str>) -> String {
    let method_body = method_body.unwrap_or(
        "match value {\n\
             0 => Reduction::Empty,\n\
             _ => Reduction::Over(15),\n\
         }",
    );
    format!(
        "#[derive(Debug, PartialEq)]\n\
         pub enum Reduction {{ Over(u32), Empty }}\n\
         pub fn consume(value: u32) -> Reduction {{\n\
             match value {{\n\
                 0 => Reduction::Empty,\n\
                 _ => Reduction::Over(16),\n\
             }}\n\
         }}\n\
         pub struct Widget;\n\
         impl Widget {{\n\
             pub fn consume(&self, value: u32) -> Reduction {{\n\
                 {method_body}\n\
             }}\n\
         }}\n\
         pub fn bridge(value: u32) -> Reduction {{ consume(value) }}\n\
         pub fn reduce(value: u32) -> Reduction {{ Reduction::Over(value) }}\n\
         #[cfg(test)]\n\
         mod tests {{\n\
             use super::*;\n\
             #[test]\n\
             fn output_contract() {{\n\
                 let widget = Widget;\n\
                 {assertion}\n\
             }}\n\
         }}\n"
    )
}

#[test]
fn same_named_call_kinds_refuse_only_wrong_owner_repo_oracles() -> Result<(), String> {
    for (owner, assertion, method_body, credited) in [
        (
            "src/lib.rs::consume",
            "assert_eq!(widget.consume(1), Reduction::Over(15));",
            None,
            false,
        ),
        (
            "src/lib.rs::impl Widget::consume",
            "assert_eq!(consume(1), Reduction::Over(16));",
            None,
            false,
        ),
        (
            "src/lib.rs::consume",
            "assert_eq!(reduce(15), Reduction::Over(15));",
            None,
            false,
        ),
        (
            "src/lib.rs::consume",
            "assert_eq!(consume(1), Reduction::Over(16));",
            None,
            true,
        ),
        (
            "src/lib.rs::consume",
            "assert_eq!(bridge(1), Reduction::Over(16));",
            None,
            true,
        ),
        (
            "src/lib.rs::consume",
            "assert_eq!(widget.consume(1), Reduction::Over(16));",
            Some("consume(value)"),
            true,
        ),
        (
            "src/lib.rs::consume",
            "assert_eq!(widget.consume(1), Reduction::Over(16));",
            Some("let run = consume; run(value)"),
            true,
        ),
        (
            "src/lib.rs::consume",
            "assert_eq!(widget.consume(1), Reduction::Over(16));",
            Some("let run = bridge; let forward = || run(value); forward()"),
            true,
        ),
        (
            "src/lib.rs::impl Widget::consume",
            "assert_eq!(widget.consume(1), Reduction::Over(15));",
            None,
            true,
        ),
        (
            "src/lib.rs::impl Widget::consume",
            "assert_eq!(Widget::consume(&widget, 1), Reduction::Over(15));",
            None,
            true,
        ),
    ] {
        let source = subject(assertion, method_body);
        let index = index_from_files(&[(PathBuf::from("src/lib.rs"), source.as_str())])?;
        let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
        let selected: Vec<_> = seams
            .iter()
            .filter(|seam| {
                seam.kind() == SeamKind::MatchArm
                    && seam.owner() == owner
                    && seam.expression() == "_ =>"
            })
            .collect();
        let [arm] = selected.as_slice() else {
            return Err(format!("expected one {owner} arm, got {}", selected.len()));
        };
        let full = evidence_for_seam(arm, &index);
        let context = CompactGripContext::new(&index);
        let compact = compact_evidence_for_seam(arm, &context);
        let expected = if credited {
            StageState::Yes
        } else {
            StageState::No
        };
        assert_eq!(full.related_tests.len(), 1, "{owner}: {assertion}");
        assert_eq!(full.observe.state, expected, "{owner}: {assertion}");
        assert_eq!(full.discriminate.state, expected, "{owner}: {assertion}");
        assert_eq!(compact.observe.state, expected, "{owner}: {assertion}");
        assert_eq!(compact.discriminate.state, expected, "{owner}: {assertion}");
        assert_eq!(
            full.related_tests[0].oracle_strength,
            if credited {
                OracleStrength::Strong
            } else {
                OracleStrength::None
            },
            "published {owner}: {assertion}"
        );
    }
    Ok(())
}

#[test]
fn same_named_call_kinds_keep_diff_owner_observation_scoped() -> Result<(), String> {
    for (owner, assertion, observed) in [
        (
            "src/lib.rs::consume",
            "assert_eq!(widget.consume(1), Reduction::Over(15));",
            false,
        ),
        (
            "src/lib.rs::impl Widget::consume",
            "assert_eq!(consume(1), Reduction::Over(16));",
            false,
        ),
        (
            "src/lib.rs::consume",
            "assert_eq!(consume(1), Reduction::Over(16));",
            true,
        ),
        (
            "src/lib.rs::impl Widget::consume",
            "assert_eq!(widget.consume(1), Reduction::Over(15));",
            true,
        ),
    ] {
        let source = subject(assertion, None);
        let index = index_from_files(&[(PathBuf::from("src/lib.rs"), source.as_str())])?;
        let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
        let arm = seams
            .iter()
            .find(|seam| {
                seam.kind() == SeamKind::MatchArm
                    && seam.owner() == owner
                    && seam.expression() == "_ =>"
            })
            .ok_or_else(|| format!("missing parsed match arm for {owner}"))?;
        let probe = Probe {
            id: ProbeId(format!("probe:12234:{owner}")),
            location: SourceLocation::new("src/lib.rs", arm.display_line(), 1),
            owner: Some(SymbolId(owner.to_string())),
            family: ProbeFamily::MatchArm,
            delta: DeltaKind::Control,
            before: None,
            after: Some("_ =>".to_string()),
            expression: "_ =>".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let finding = classify_probe(&probe, &index, true, None);
        assert_eq!(finding.related_tests.len(), 1, "{owner}: {assertion}");
        assert_eq!(
            finding.ripr.reveal.observe.state,
            if observed {
                StageState::Yes
            } else {
                StageState::No
            },
            "diff {owner}: {assertion}"
        );
    }
    Ok(())
}

#[test]
fn same_named_call_kinds_preserve_real_wrapper_and_binding_paths() -> Result<(), String> {
    for (case, owner, source) in [
        (
            "plain",
            "src/lib.rs::consume",
            r##"#[derive(Debug, PartialEq)]
pub enum Reduction { Over(u32), Empty }
pub fn consume(value: u32) -> Reduction {
    match value {
        0 => Reduction::Empty,
        _ => Reduction::Over(16),
    }
}
type Operation = fn(u32) -> Reduction;
pub fn forward() -> Operation { consume }
pub struct Widget;
impl Widget {
    pub fn consume(&self, value: u32) -> Reduction { forward()(value) }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_contract() {
        let widget = Widget;
        assert_eq!(widget.consume(1), Reduction::Over(16));
    }
}
"##,
        ),
        (
            "generic",
            "src/lib.rs::consume",
            r##"#[derive(Debug, PartialEq)]
pub enum Reduction { Over(u32), Empty }
pub fn consume<T>(value: u32) -> Reduction {
    match value {
        0 => Reduction::Empty,
        _ => Reduction::Over(16),
    }
}
type Operation = fn(u32) -> Reduction;
pub fn forward() -> Operation { consume::<u32> }
pub struct Widget;
impl Widget {
    pub fn consume(&self, value: u32) -> Reduction { forward()(value) }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_contract() {
        let widget = Widget;
        assert_eq!(widget.consume(1), Reduction::Over(16));
    }
}
"##,
        ),
        (
            "unicode_lrm",
            "src/lib.rs::consume",
            concat!(
                r##"#[derive(Debug, PartialEq)]
pub enum Reduction { Over(u32), Empty }
pub fn consume(value: u32) -> Reduction {
    match value {
        0 => Reduction::Empty,
        _ => Reduction::Over(16),
    }
}
type Operation = fn(u32) -> Reduction;
pub fn forward() -> Operation { "##,
                "\u{200e}",
                r##"consume"##,
                "\u{200e}",
                r##" }
pub struct Widget;
impl Widget {
    pub fn consume(&self, value: u32) -> Reduction { forward()(value) }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_contract() {
        let widget = Widget;
        assert_eq!(widget.consume(1), Reduction::Over(16));
    }
}
"##,
            ),
        ),
        (
            "unicode_line_separator",
            "src/lib.rs::consume",
            concat!(
                r##"#[derive(Debug, PartialEq)]
pub enum Reduction { Over(u32), Empty }
pub fn consume(value: u32) -> Reduction {
    match value {
        0 => Reduction::Empty,
        _ => Reduction::Over(16),
    }
}
type Operation = fn(u32) -> Reduction;
pub fn forward() -> Operation { "##,
                "\u{2028}",
                r##"consume"##,
                "\u{2028}",
                r##" }
pub struct Widget;
impl Widget {
    pub fn consume(&self, value: u32) -> Reduction { forward()(value) }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_contract() {
        let widget = Widget;
        assert_eq!(widget.consume(1), Reduction::Over(16));
    }
}
"##,
            ),
        ),
        (
            "raw_helper",
            "src/lib.rs::consume",
            r##"#[derive(Debug, PartialEq)]
pub enum Reduction { Over(u32), Empty }
pub fn consume(value: u32) -> Reduction {
    match value {
        0 => Reduction::Empty,
        _ => Reduction::Over(16),
    }
}
type Operation = fn(u32) -> Reduction;
pub fn r#bridge(value: u32) -> Reduction { consume(value) }
pub fn forward() -> Operation { r#bridge }
pub struct Widget;
impl Widget {
    pub fn consume(&self, value: u32) -> Reduction { forward()(value) }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_contract() {
        let widget = Widget;
        assert_eq!(widget.consume(1), Reduction::Over(16));
    }
}
"##,
        ),
        (
            "unicode_nfc_helper",
            "src/lib.rs::consume",
            r##"#[derive(Debug, PartialEq)]
pub enum Reduction { Over(u32), Empty }
pub fn consume(value: u32) -> Reduction {
    match value {
        0 => Reduction::Empty,
        _ => Reduction::Over(16),
    }
}
type Operation = fn(u32) -> Reduction;
pub fn café(value: u32) -> Reduction { consume(value) }
pub fn forward() -> Operation { café }
pub struct Widget;
impl Widget {
    pub fn consume(&self, value: u32) -> Reduction { forward()(value) }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_contract() {
        let widget = Widget;
        assert_eq!(widget.consume(1), Reduction::Over(16));
    }
}
"##,
        ),
        (
            "unindexed_std_receiver",
            "src/lib.rs::next",
            r##"#[derive(Debug, PartialEq)]
pub enum Reduction { Over(u32), Empty }
pub fn next() -> Reduction {
    match 1 {
        0 => Reduction::Empty,
        _ => Reduction::Over(16),
    }
}
pub struct Widget;
impl Widget {
    pub fn next(&mut self) -> Option<Reduction> { Some(Reduction::Over(15)) }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_contract() {
        let mut external = std::iter::once_with(next);
        assert_eq!(external.next(), Some(Reduction::Over(16)));
    }
}
"##,
        ),
        (
            "uppercase_callable",
            "src/lib.rs::consume",
            r##"#[derive(Debug, PartialEq)]
pub enum Reduction { Over(u32), Empty }
pub fn consume(value: u32) -> Reduction {
    match value {
        0 => Reduction::Empty,
        _ => Reduction::Over(16),
    }
}
type Operation = fn(u32) -> Reduction;
pub struct Widget;
impl Widget {
    #[allow(non_snake_case)]
    pub fn consume(&self, Run: Operation) -> Reduction { Run(1) }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_contract() {
        let widget = Widget;
        let run = consume;
        assert_eq!(widget.consume(run), Reduction::Over(16));
    }
}
"##,
        ),
        (
            "shadowed_free_callable",
            "src/lib.rs::impl Widget::consume",
            r##"#[derive(Debug, PartialEq)]
pub enum Reduction { Over(u32), Empty }
pub fn consume(value: u32) -> Reduction { Reduction::Over(15 + value) }
pub struct Widget;
impl Widget {
    pub fn consume(&self, value: u32) -> Reduction {
        match value {
            0 => Reduction::Empty,
            _ => Reduction::Over(16),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_contract() {
        let widget = Widget;
        let consume = |v| widget.consume(v);
        assert_eq!(consume(1), Reduction::Over(16));
    }
}
"##,
        ),
        (
            "parenthesized_callable",
            "src/lib.rs::consume",
            r##"#[derive(Debug, PartialEq)]
pub enum Reduction { Over(u32), Empty }
pub fn consume(value: u32) -> Reduction {
    match value {
        0 => Reduction::Empty,
        _ => Reduction::Over(16),
    }
}
type Operation = fn(u32) -> Reduction;
pub struct Widget;
impl Widget {
    #[allow(non_snake_case)]
    pub fn consume(&self, Run: Operation) -> Reduction { (Run)(1) }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_contract() {
        let widget = Widget;
        let run = consume;
        assert_eq!(widget.consume(run), Reduction::Over(16));
    }
}
"##,
        ),
        (
            "constructor_method_name_collision",
            "src/lib.rs::consume",
            r##"#[derive(Debug, PartialEq)]
pub enum Reduction { Over(u32), Empty }
pub fn consume(value: u32) -> Reduction {
    match value {
        0 => Reduction::Empty,
        _ => Reduction::Over(16),
    }
}
pub struct Runner { callback: fn(u32) -> Reduction }
impl Runner {
    #[allow(non_snake_case)]
    pub fn Over(&self, value: u32) -> Reduction { (self.callback)(value) }
}
pub struct Widget;
impl Widget {
    pub fn consume(&self, runner: &Runner, value: u32) -> Reduction {
        let _unused = Reduction::Over(15);
        runner.Over(value)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_contract() {
        let widget = Widget;
        let runner = Runner { callback: consume };
        assert_eq!(widget.consume(&runner, 1), Reduction::Over(16));
    }
}
"##,
        ),
        (
            "constructor_module_shadow_local_test",
            "src/lib.rs::consume",
            r##"#[derive(Debug, PartialEq)]
pub enum Reduction { Over(u32), Empty }
pub fn consume(value: u32) -> Reduction {
    match value {
        0 => Reduction::Empty,
        _ => Reduction::Over(16),
    }
}
#[allow(non_snake_case)]
pub mod other {
    pub mod Reduction {
        pub fn Over(value: u32) -> crate::Reduction { crate::consume(value) }
    }
    pub struct Widget;
    impl Widget {
        pub fn consume(&self, value: u32) -> crate::Reduction { Reduction::Over(value) }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn receiver_contract() {
            let widget = Widget;
            assert_eq!(widget.consume(1), crate::Reduction::Over(16));
        }
    }
}
"##,
        ),
        (
            "rewriting_method_attribute",
            "src/lib.rs::consume",
            r##"#[derive(Debug, PartialEq)]
pub enum Reduction { Over(u32), Empty }

pub fn consume(value: u32) -> Reduction {
    match value {
        0 => Reduction::Empty,
        _ => Reduction::Over(16),
    }
}

pub struct Widget;
impl Widget {
    #[owner_macros::forward]
    pub fn consume(&self, value: u32) -> Reduction {
        match value {
            0 => Reduction::Empty,
            _ => Reduction::Empty,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_contract() {
        let widget = Widget;
        assert_eq!(widget.consume(1), Reduction::Over(16));
    }
}
"##,
        ),
        (
            "const_owner_result",
            "src/lib.rs::consume",
            r##"#[derive(Debug, PartialEq)]
pub enum Reduction { Over(u32), Empty }

pub const fn consume(value: u32) -> Reduction {
    match value {
        0 => Reduction::Empty,
        _ => Reduction::Over(16),
    }
}

const EXPECTED: Reduction = consume(1);

pub struct Widget;
impl Widget {
    pub fn consume(&self) -> Reduction {
        EXPECTED
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_contract() {
        let widget = Widget;
        assert_eq!(widget.consume(), Reduction::Over(16));
    }
}
"##,
        ),
        (
            "assertion_const_value_flow",
            "src/lib.rs::consume",
            r##"pub const fn consume(value: u32) -> u32 {
    match value {
        0 => 0,
        _ => 16,
    }
}

const ACTUAL: u32 = consume(1);

pub struct Widget;
impl Widget {
    pub fn consume(&self, value: u32) -> u32 {
        match value {
            0 => 0,
            _ => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_contract() {
        let widget = Widget;
        assert_eq!(widget.consume(1) + ACTUAL, 17);
    }
}
"##,
        ),
        (
            "assertion_equality_const_flow",
            "src/lib.rs::consume",
            r##"#[derive(Debug)]
pub enum Reduction { Over(u32), Empty }

pub const fn consume(value: u32) -> u32 {
    match value {
        0 => 0,
        _ => 16,
    }
}

const ACTUAL: u32 = consume(1);

impl PartialEq for Reduction {
    fn eq(&self, _other: &Self) -> bool {
        ACTUAL == 16
    }
}

pub struct Widget;
impl Widget {
    pub fn consume(&self, value: u32) -> Reduction {
        match value {
            0 => Reduction::Empty,
            _ => Reduction::Over(15),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_contract() {
        let widget = Widget;
        assert_eq!(widget.consume(1), Reduction::Over(15));
    }
}
"##,
        ),
    ] {
        let index = index_from_files(&[(PathBuf::from("src/lib.rs"), source)])?;
        let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
        let selected: Vec<_> = seams
            .iter()
            .filter(|seam| {
                seam.kind() == SeamKind::MatchArm
                    && seam.owner() == owner
                    && seam.expression() == "_ =>"
            })
            .collect();
        let [arm] = selected.as_slice() else {
            return Err(format!(
                "{case}: expected one selected arm, got {}",
                selected.len()
            ));
        };
        let full = evidence_for_seam(arm, &index);
        let context = CompactGripContext::new(&index);
        let compact = compact_evidence_for_seam(arm, &context);
        assert_eq!(full.related_tests.len(), 1, "{case}: nonempty test stimulus");
        assert_eq!(full.observe.state, StageState::Yes, "{case}: full observe");
        assert_eq!(
            full.discriminate.state,
            StageState::Yes,
            "{case}: full discriminate"
        );
        assert_eq!(
            full.related_tests[0].oracle_strength,
            OracleStrength::Strong,
            "{case}: oracle"
        );
        assert_eq!(
            compact.observe.state,
            StageState::Yes,
            "{case}: compact observe"
        );
        assert_eq!(
            compact.discriminate.state,
            StageState::Yes,
            "{case}: compact discriminate"
        );
    }
    Ok(())
}
