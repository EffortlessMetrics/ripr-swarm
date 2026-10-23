export const sink = {
  write(line: string): void {
    void line;
  },
};

export function record(event: string): string {
  const line = `event:${event}`;
  sink.write(line);
  return line;
}
