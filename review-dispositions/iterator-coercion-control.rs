pub fn macro_binding_scan_needed(source: &str, trusted: &[&str]) -> bool {
    trusted.contains(&"json") && source.contains("serde_json")
        || source.contains("macro") || source.contains("use")
        || source.contains("no_implicit_prelude") || source.contains('!')
}
pub fn unparsed_shape(source: &str, singleton_names: Option<&[&str]>) -> Vec<String> {
    singleton_names.into_iter().flatten()
        .filter(|name| macro_binding_scan_needed(source, &[name]))
        .map(|name| (*name).to_string()).collect()
}
pub fn parsed_shape(source: &str, singleton_names: Option<&[&str]>) -> Vec<String> {
    let scan = |names: &[&str]| -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    };
    singleton_names.into_iter().flatten()
        .filter(|name| macro_binding_scan_needed(source, &[name]))
        .flat_map(|name| scan(&[name])).collect()
}
