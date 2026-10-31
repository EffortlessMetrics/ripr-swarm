export function shadowedConstant(score: number): string {
    if (score > 50) {
        return "gold";
    }
    return "basic";
}
