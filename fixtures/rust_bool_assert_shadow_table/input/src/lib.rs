pub fn can_ship(paid: bool, in_stock: bool) -> bool {
    in_stock && paid
}
