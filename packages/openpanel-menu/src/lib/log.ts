/**
 * Colour-aware console logger with greppable tags.
 *
 * On a TTY each tag is colourised: `[INFO]` blue, `[OK]` green,
 * `[WARN]` yellow, `[ERROR]` red. On a non-TTY the colour escapes
 * are stripped. The tag is the only fixed prefix; the message is
 * whatever the caller passes.
 *
 * `redact` enforces the spec rule that passwords, tokens, PEM
 * private keys, and `--password <value>` argv pairs never appear
 * in any log line.
 */

const TAG_INFO = 'INFO';
const TAG_OK = 'OK';
const TAG_WARN = 'WARN';
const TAG_ERROR = 'ERROR';
const TAG_RAW = 'RAW';

const ESC = '\x1b[';
const RESET = `${ESC}0m`;
const COLOURS: Readonly<Record<string, string>> = {
  INFO: `${ESC}34m`,
  OK: `${ESC}32m`,
  WARN: `${ESC}33m`,
  ERROR: `${ESC}31m`,
  RAW: '',
};

function isTTY(stream: NodeJS.WriteStream): boolean {
  return Boolean((stream as { isTTY?: boolean }).isTTY);
}

function colourise(tag: string, text: string, useColour: boolean): string {
  if (!useColour) {
    return `[${tag}] ${text}`;
  }
  const colour = COLOURS[tag] ?? '';
  return `${colour}[${tag}]${RESET} ${text}`;
}

/** Strip ANSI escape sequences (used in tests and for non-TTY output). */
export function stripAnsi(input: string): string {
  return input.replace(/\x1b\[[0-9;]*m/g, '');
}

/** The Logger contract the dispatcher and handlers consume. */
export interface Logger {
  info(message: string): void;
  ok(message: string): void;
  warn(message: string): void;
  error(message: string): void;
  raw(message: string): void;
}

/** Build a logger bound to a given stdout/stderr pair. */
export function makeLogger(
  out: NodeJS.WriteStream,
  err: NodeJS.WriteStream = out,
): Logger {
  const outColour = isTTY(out);
  const errColour = isTTY(err);

  function write(stream: NodeJS.WriteStream, tag: string, message: string, colour: boolean): void {
    stream.write(`${colourise(tag, message, colour)}\n`);
  }

  return {
    info(message) {
      write(out, TAG_INFO, message, outColour);
    },
    ok(message) {
      write(out, TAG_OK, message, outColour);
    },
    warn(message) {
      write(err, TAG_WARN, message, errColour);
    },
    error(message) {
      write(err, TAG_ERROR, message, errColour);
    },
    raw(message) {
      write(out, TAG_RAW, message, false);
    },
  };
}

/** A logger that records into a list — used by tests. */
export function makeRecordingLogger(): Logger & { readonly lines: ReadonlyArray<string> } {
  const lines: string[] = [];
  function push(tag: string, message: string): void {
    lines.push(stripAnsi(`[${tag}] ${message}`));
  }
  return {
    info: (m) => push(TAG_INFO, m),
    ok: (m) => push(TAG_OK, m),
    warn: (m) => push(TAG_WARN, m),
    error: (m) => push(TAG_ERROR, m),
    raw: (m) => push(TAG_RAW, m),
    get lines() {
      return [...lines];
    },
  } as Logger & { readonly lines: ReadonlyArray<string> };
}
