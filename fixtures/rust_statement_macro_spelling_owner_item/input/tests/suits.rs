use rust_statement_macro_spelling_owner_item::suit_color;

#[test]
fn spades_are_black() {
    rust_statement_macro_spelling_owner_item::emit!(fn suit_color(_s: char) -> &'static str { "black" });
    assert_eq!(suit_color('S'), "black");
}
