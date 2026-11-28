const { Cart } = require('../src/cart');

describe('Cart', () => {
  test('takes five off at the threshold', () => {
    expect(new Cart().total(50)).toBe(45);
  });
});
