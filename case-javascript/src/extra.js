export function extraDiscount(amount, threshold) {
    if (amount >= threshold) {
        return amount - 10;
    }
    return amount;
}
