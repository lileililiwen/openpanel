/**
 * Typed wrapper around `child_process.spawn`.
 *
 * Every subprocess the menu runs goes through this module. The
 * spec mandates:
 *  - `shell: false` so argv is never parsed by /bin/sh.
 *  - stdout and stderr captured separately.
 *  - Non-zero exits are reported via `ok=false` (not thrown).
 *  - Missing binaries throw a typed `ExecError`.
 */

import { spawn } from 'node:child_process';
import process from 'node:process';

/** Result of one subprocess invocation. */
export interface ExecResult {
  readonly ok: boolean;
  readonly code: number | null;
  readonly signal: NodeJS.Signals | null;
  readonly stdout: string;
  readonly stderr: string;
  readonly argv: ReadonlyArray<string>;
}

/** A typed error thrown when the requested binary is missing. */
export class ExecError extends Error {
  readonly argv: ReadonlyArray<string>;
  constructor(message: string, argv: ReadonlyArray<string>) {
    super(message);
    this.name = 'ExecError';
    this.argv = argv;
  }
}

/**
 * Run a subprocess and resolve with its result. The promise NEVER
 * rejects on non-zero exit; callers must inspect `ok`.
 */
export function run(argv: ReadonlyArray<string>, opts: { readonly cwd?: string } = {}): Promise<ExecResult> {
  if (argv.length === 0) {
    return Promise.reject(new ExecError('argv is empty', argv));
  }
  const cmd = argv[0];
  if (cmd === undefined) {
    return Promise.reject(new ExecError('argv[0] is undefined', argv));
  }
  return new Promise<ExecResult>((resolve, reject) => {
    let stdout = '';
    let stderr = '';
    let settled = false;

    let child;
    try {
      child = spawn(cmd, argv.slice(1), {
        shell: false,
        stdio: ['ignore', 'pipe', 'pipe'],
        cwd: opts.cwd ?? process.cwd(),
      });
    } catch (err) {
      settled = true;
      reject(err instanceof Error ? err : new Error(String(err)));
      return;
    }

    child.on('error', (err) => {
      if (settled) {
        return;
      }
      settled = true;
      const code = (err as NodeJS.ErrnoException).code;
      if (code === 'ENOENT') {
        reject(new ExecError(`binary not found: ${cmd}`, [...argv]));
      } else {
        reject(err);
      }
    });

    child.stdout?.on('data', (chunk: Buffer | string) => {
      stdout += chunk.toString();
    });
    child.stderr?.on('data', (chunk: Buffer | string) => {
      stderr += chunk.toString();
    });

    child.on('close', (code, signal) => {
      if (settled) {
        return;
      }
      settled = true;
      resolve({
        ok: code === 0,
        code,
        signal,
        stdout,
        stderr,
        argv: [...argv],
      });
    });
  });
}

/**
 * The port the dispatcher uses for `systemctl` and `openpanel-cli`
 * — both must be reachable on `PATH`. Tests inject a stub.
 */
export interface ExecLike {
  readonly run: (argv: ReadonlyArray<string>, opts?: { readonly cwd?: string }) => Promise<ExecResult>;
}

/** Default `ExecLike` for production: delegates to `run`. */
export function makeShellExec(): ExecLike {
  return { run };
}
