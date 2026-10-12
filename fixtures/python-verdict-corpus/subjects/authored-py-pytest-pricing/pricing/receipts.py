"""Receipt printing."""


def print_receipt(total_cents):
    """Print the order total in dollars."""
    print(f"Total: {total_cents / 100:.2f}")


def receipt_footer(store_name):
    """Closing line printed under every receipt."""
    return f"Thank you for shopping at {store_name}!"
