export function emit(sink: { write(event: string): void }, event: string): void {
  void event;
}
