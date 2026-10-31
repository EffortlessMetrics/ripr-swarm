export function offValueConstant(score: number): string {
    if (score > 50) {
        return "gold";
    }
    return "basic";
}
