/**
 * `stty -echo` wrapper used by `promptHidden`.
 *
 * On a real TTY, `stty -echo` disables the terminal echo so the
 * typed password is not printed. On a non-TTY, the caller falls
 * back to a visible read with a warning.
 */

import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);

/** Result of toggling the terminal echo. */
export interface SttyResult {
  readonly ok: boolean;
  readonly stdout: string;
  readonly stderr: string;
}

/** Run `stty -echo` to disable terminal echo. */
export async function sttyEchoOff(): Promise<SttyResult> {
  try {
    const { stdout, stderr } = await execFileAsync('stty', ['-echo'], { shell: false });
    return { ok: true, stdout, stderr };
  } catch (err) {
    const e = err as { stdout?: string; stderr?: string; message?: string };
    return { ok: false, stdout: e.stdout ?? '', stderr: e.stderr ?? e.message ?? '' };
  }
}

/** Run `stty echo` to re-enable terminal echo. */
export async function sttyEchoOn(): Promise<SttyResult> {
  try {
    const { stdout, stderr } = await execFileAsync('stty', ['echo'], { shell: false });
    return { ok: true, stdout, stderr };
  } catch (err) {
    const e = err as { stdout?: string; stderr?: string; message?: string };
    return { ok: false, stdout: e.stdout ?? '', stderr: e.stderr ?? e.message ?? '' };
  }
}

/** True if the current process has a TTY on stdin. */
export function hasTty(): boolean {
  return Boolean((process.stdin as { isTTY?: boolean }).isTTY);
}
