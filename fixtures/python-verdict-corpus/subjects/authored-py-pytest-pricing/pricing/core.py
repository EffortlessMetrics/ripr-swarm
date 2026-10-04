"""Pricing rules: shipping, loyalty tiers, discounts and tax."""

FREE_SHIPPING_CENTS = 5000
FLAT_SHIPPING_CENTS = 499


def shipping_fee(subtotal_cents):
    """Orders at or above the free-shipping threshold ship free."""
    if subtotal_cents >= FREE_SHIPPING_CENTS:
        return 0
    return FLAT_SHIPPING_CENTS


def tier_for_points(points):
    """Loyalty tier earned by accumulated points."""
    if points >= 5000:
        return "gold"
    if points >= 1000:
        return "silver"
    return "bronze"


def discount_cents(tier, subtotal_cents):
    """Discount a tier earns on a subtotal."""
    if tier == "gold":
        return subtotal_cents * 10 // 100
    if tier == "silver":
        return subtotal_cents * 5 // 100
    return 0


def tax_cents(subtotal_cents):
    """Sales tax on a subtotal, rounded down."""
    return subtotal_cents * 8 // 100


def total_with_tax(subtotal_cents):
    """Subtotal plus its sales tax."""
    return subtotal_cents + tax_cents(subtotal_cents)


def parse_quantity(text):
    """Parse a positive order quantity."""
    value = int(text)
    if value <= 0:
        raise ValueError("quantity must be positive")
    return value


def loyalty_points(subtotal_cents):
    """Points earned for a subtotal: one per whole dollar."""
    return subtotal_cents // 100


def label_for(tier):
    """Display label for a tier."""
    return tier.upper()


def bulk_price(unit_cents, quantity, bulk_threshold=10):
    """Price for a quantity; bulk orders get ten percent off."""
    if quantity >= bulk_threshold:
        return unit_cents * quantity * 9 // 10
    return unit_cents * quantity


def quote(subtotal_cents, points):
    """Tier and total for an order."""
    tier = tier_for_points(points)
    total = subtotal_cents - discount_cents(tier, subtotal_cents)
    return {"tier": tier, "total_cents": total + shipping_fee(subtotal_cents)}
