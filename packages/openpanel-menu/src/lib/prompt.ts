/**
 * Readline + stty-echo helpers used by the menu loop.
 *
 * The spec demands:
 *  - `promptLine` returns trimmed input or a default if empty.
 *  - `promptConfirm` parses `y` / `yes` / `n` / `no` with a default.
 *  - `promptHidden` uses `stty -echo` on a real TTY and falls
 *    back to a visible read with a warning on a non-TTY.
 *
 * The implementation does NOT use `node:readline/promises`
 * because piping a finite stream (`echo a; echo b | opctl`)
 * into a `createInterface` does not always deliver subsequent
 * lines after the first read settles (a long-standing Node.js
 * limitation with finite piped streams). Instead, we accumulate
 * data on the underlying stream ourselves and split on `\n`.
 *
 * The `readlineFactory` indirection lets tests inject a stub
 * `Interface` so we never need a real TTY or a real stdin.
 */

import { createInterface, type Interface } from 'node:readline/promises';
import { stdin, stdout } from 'node:process';
import { hasTty, sttyEchoOff, sttyEchoOn } from './stty.js';
import type { Logger } from './log.js';

export interface PromptLineOptions {
  readonly defaultValue?: string;
}

/** A factory that produces a readline interface. Tests inject a stub. */
export type ReadlineFactory = () => Interface;

let readlineFactory: ReadlineFactory = defaultReadlineFactory();

/** Replace the readline factory — only used by tests. */
export function setReadlineFactory(factory: ReadlineFactory): void {
  readlineFactory = factory;
}

/** Reset to the production factory. */
export function resetReadlineFactory(): void {
  readlineFactory = defaultReadlineFactory();
}

/** The default factory is rarely used because `PromptSession`
 *  manages its own line accumulator. Kept for compatibility. */
function defaultReadlineFactory(): () => Interface {
  return () =>
    createInterface({ input: stdin, output: stdout, terminal: hasTty() });
}

/** True once stdin has emitted 'end' (EOF reached). */
let stdinEnded = false;
if (stdin && typeof stdin.on === 'function') {
  stdin.on('end', () => {
    stdinEnded = true;
  });
  stdin.on('close', () => {
    stdinEnded = true;
  });
}

/** Return (and reset) the EOF flag. Used by the dispatcher to exit cleanly. */
export function consumeStdinEnded(): boolean {
  const v = stdinEnded;
  stdinEnded = false;
  return v;
}

/** Read a line of input. Returns the trimmed value or the default if the user typed empty. */
export async function promptLine(
  message: string,
  opts: PromptLineOptions = {},
  log: Logger,
): Promise<string> {
  const session = new PromptSession(readlineFactory());
  try {
    return await session.question(message, opts, log);
  } finally {
    session.close();
  }
}

/**
 * Confirm with `y` / `yes` / `n` / `no`. Default is the second
 * argument; matching is case-insensitive and lenient on whitespace.
 */
export async function promptConfirm(
  message: string,
  defaultYes: boolean = false,
  log: Logger,
): Promise<boolean> {
  const session = new PromptSession(readlineFactory());
  try {
    return await session.confirm(message, defaultYes, log);
  } finally {
    session.close();
  }
}

/** Read a password with TTY masking. Falls back to visible on a non-TTY. */
export async function promptHidden(
  message: string,
  log: Logger,
): Promise<string> {
  if (!hasTty()) {
    log.warn('警告: 无法屏蔽输入回显. 密码将以明文显示.');
    const session = new PromptSession(readlineFactory());
    try {
      return await session.hidden(message, log);
    } finally {
      session.close();
    }
  }
  log.raw(`${message}: `);
  const off = await sttyEchoOff();
  if (!off.ok) {
    log.warn('警告: 无法屏蔽输入回显. 密码将以明文显示.');
  }
  const session = new PromptSession(readlineFactory());
  try {
    return await session.hidden(message, log);
  } finally {
    await sttyEchoOn();
    stdout.write('\n');
    session.close();
  }
}

/**
 * A persistent prompt session. Production code uses this for the
 * menu loop so consecutive `question()` calls share the same
 * buffered stdin.
 *
 * The class manually accumulates bytes from `process.stdin` and
 * splits on `\n` (and `\r`) so the implementation does NOT rely
 * on the broken `readline.createInterface` + piped-stdin path.
 *
 * On a TTY, the implementation emits one character at a time
 * (no real key handling — masked prompts use `stty -echo` so the
 * typed character is not echoed to the screen). On a non-TTY,
 * the implementation reads from `process.stdin` directly.
 *
 * Tests inject a stub interface (the legacy `Interface` shape
 * with `question()`) so they can supply scripted answers without
 * touching the real stdin.
 */
export class PromptSession {
  private closed = false;
  private buffer = '';
  private stdinDataCb: ((chunk: string | Buffer) => void) | null = null;
  private stdinEndCb: (() => void) | null = null;
  private waiter: { resolve: (line: string) => void; reject: (err: Error) => void } | null = null;
  private ended = false;
  private readonly useTty: boolean;
  private readonly stub: Interface | null;

  constructor(rl?: Interface) {
    this.useTty = hasTty();
    this.stub = rl ?? null;
    if (this.stub !== null) {
      return;
    }
    if (stdin && typeof stdin.on === 'function' && !this.useTty) {
      // Non-TTY: hook data and end events on stdin directly.
      const onData = (chunk: string | Buffer): void => {
        const text = typeof chunk === 'string' ? chunk : chunk.toString('utf8');
        this.buffer += text;
        this.drain();
      };
      const onEnd = (): void => {
        this.ended = true;
        this.drain(true);
      };
      stdin.on('data', onData);
      stdin.on('end', onEnd);
      this.stdinDataCb = onData;
      this.stdinEndCb = onEnd;
    }
  }

  private drain(force = false): void {
    if (this.waiter === null) {
      return;
    }
    const idx = this.buffer.indexOf('\n');
    if (idx >= 0) {
      const line = this.buffer.slice(0, idx).replace(/\r$/, '');
      this.buffer = this.buffer.slice(idx + 1);
      const w = this.waiter;
      this.waiter = null;
      w.resolve(line);
      return;
    }
    if (force && this.ended) {
      const line = this.buffer;
      this.buffer = '';
      const w = this.waiter;
      this.waiter = null;
      w.resolve(line);
    }
  }

  /** Wait for the next line. */
  private nextLine(): Promise<string> {
    if (this.stub !== null) {
      // Use the stub interface: each call to question() returns the
      // next scripted answer.
      const stubbed = this.stub.question('') as Promise<string>;
      return stubbed;
    }
    if (this.ended && !this.buffer.includes('\n')) {
      return Promise.resolve(this.buffer);
    }
    return new Promise<string>((resolve, reject) => {
      this.waiter = { resolve, reject };
      this.drain(this.ended);
    });
  }

  /** Read a line of input with a prompt message. */
  async question(message: string, opts: PromptLineOptions = {}, log: Logger): Promise<string> {
    if (this.closed) {
      throw new Error('PromptSession is closed');
    }
    log.raw(`${message}${opts.defaultValue !== undefined ? ` [${opts.defaultValue}]` : ''}: `);
    const raw = await this.nextLine();
    const trimmed = raw.trim();
    if (trimmed === '' && opts.defaultValue !== undefined) {
      return opts.defaultValue;
    }
    return trimmed;
  }

  /** Read a confirm. */
  async confirm(message: string, defaultYes: boolean, log: Logger): Promise<boolean> {
    if (this.closed) {
      throw new Error('PromptSession is closed');
    }
    const hint = defaultYes ? 'Y/n' : 'y/N';
    log.raw(`${message} (${hint}): `);
    const answer = (await this.nextLine()).trim().toLowerCase();
    if (answer === '') {
      return defaultYes;
    }
    if (answer === 'y' || answer === 'yes') {
      return true;
    }
    if (answer === 'n' || answer === 'no') {
      return false;
    }
    return defaultYes;
  }

  /** Read a password (one line, possibly with masking on a TTY). */
  async hidden(_message: string, _log: Logger): Promise<string> {
    if (this.closed) {
      throw new Error('PromptSession is closed');
    }
    const line = await this.nextLine();
    return line.replace(/\r?\n$/, '');
  }

  close(): void {
    if (this.closed) {
      return;
    }
    this.closed = true;
    if (this.stdinDataCb && stdin && typeof stdin.off === 'function') {
      stdin.off('data', this.stdinDataCb);
    }
    if (this.stdinEndCb && stdin && typeof stdin.off === 'function') {
      stdin.off('end', this.stdinEndCb);
    }
    this.stdinDataCb = null;
    this.stdinEndCb = null;
    if (this.stub) {
      this.stub.close();
    }
    if (this.waiter) {
      this.waiter.resolve('');
      this.waiter = null;
    }
  }
}
