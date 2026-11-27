pub(crate) fn check_allow_attributes() -> Result<(), String> {
    crate::check_allow_attributes_impl()
}

#[cfg(test)]
mod tests {
    use crate::{guarded_allow_attribute_lints, guarded_allow_attributes_in_text};

    /// A file that ends inside an attribute name is read up to its last byte
    /// and no further: no finding and no out-of-bounds read, whichever
    /// attribute kind is cut off.
    #[test]
    fn detection_stops_at_end_of_text_inside_an_attribute_name() {
        for text in [
            concat!("#", "[allow"),
            concat!("#", "![expect"),
            concat!("#", " [ cfg_attr"),
            concat!("fn f() {}\n#", "[al"),
        ] {
            assert_eq!(
                guarded_allow_attributes_in_text(text, &guarded_allow_attribute_lints()),
                Vec::new(),
                "{text:?}"
            );
        }
    }
}
