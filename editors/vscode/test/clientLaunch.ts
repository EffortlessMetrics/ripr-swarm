import * as cp from 'child_process';

/** One test-only process owner for exact client-launch argv and shell controls. */
export function spawnClientProcess(command: string, args: readonly string[], useShell: boolean) {
  return cp.spawn(command, args, { shell: useShell });
}
