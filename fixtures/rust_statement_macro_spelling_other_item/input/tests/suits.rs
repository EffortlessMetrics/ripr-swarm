use rust_statement_macro_spelling_other_item::suit_color;

#[test]
fn spades_are_black() {
    rust_statement_macro_spelling_other_item::emit!(fn other_name(_s: char) -> &'static str { "black" });
    assert_eq!(suit_color('S'), "black");
}
