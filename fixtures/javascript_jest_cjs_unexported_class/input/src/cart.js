class Cart {
  total(n) {
    if (n >= 50) {
      return n - 5;
    }
    return n;
  }
}
exports.Basket = Cart;
