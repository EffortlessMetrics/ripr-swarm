export function check(field: string): void {
    if (field === "read") {
        throw new RangeError("read limit exceeded");
    }
    if (field === "write") {
        throw new RangeError("write limit exceeded");
    }
    if (field === "connect") {
        throw new RangeError("connect limit exceeded");
    }
    if (field === "idle") {
        throw new RangeError("idle limit exceeded");
    }
    if (field === "retry") {
        throw new RangeError("retry limit exceeded");
    }
    if (field === "burst") {
        throw new RangeError("burst limit exceeded");
    }
    if (field === "flush") {
        throw new RangeError("flush limit exceeded");
    }
    if (field === "sync") {
        throw new RangeError("sync limit exceeded");
    }
    if (field === "open") {
        throw new RangeError("open limit exceeded");
    }
    if (field === "close") {
        throw new RangeError("close limit exceeded");
    }
    if (field === "seek") {
        throw new RangeError("seek limit exceeded");
    }
    if (field === "poll") {
        throw new RangeError("poll limit exceeded");
    }
}
