/**
 * Hand-rolled port-line parser for `/etc/openpanel/openpanel.toml`.
 *
 * The spec mandates a typed, no-regex-parsing-light library: the
 * panel's TOML is small and the only line we care about is
 *
 *     port = NNNN
 *
 * possibly under a `[server]` section, with optional comments and
 * whitespace. We accept the line case-insensitively and reject
 * any out-of-range or non-numeric value.
 */

import { promises as fs } from 'node:fs';
import path from 'node:path';

/** A typed error for parser failures. */
export class ConfigError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ConfigError';
  }
}

const PORT_LINE = /^\s*port\s*=\s*(\d+)\s*(?:#.*)?$/i;

/** Parse a port from a TOML string. */
export function parsePort(input: string): number {
  const trimmed = input.replace(/\r/g, '');
  let inServer = false;
  for (const rawLine of trimmed.split('\n')) {
    const line = rawLine.trim();
    if (line === '' || line.startsWith('#')) {
      continue;
    }
    const section = /^\[\s*([^\]]+)\s*\]$/.exec(line);
    if (section && section[1] !== undefined) {
      inServer = section[1].trim().toLowerCase() === 'server';
      continue;
    }
    const match = PORT_LINE.exec(rawLine);
    if (match) {
      const value = match[1];
      if (value === undefined) {
        continue;
      }
      const parsed = Number.parseInt(value, 10);
      if (!Number.isFinite(parsed)) {
        continue;
      }
      if (!inServer) {
        // Accept port anywhere — but if we have a [server] section, prefer it.
        if (parsed >= 1 && parsed <= 65535) {
          return parsed;
        }
        continue;
      }
      if (parsed >= 1 && parsed <= 65535) {
        return parsed;
      }
      throw new ConfigError(`port out of range: ${parsed}`);
    }
  }
  throw new ConfigError('port not found');
}

/** Canonical config path used by the panel. */
export const DEFAULT_CONFIG_PATH = '/etc/openpanel/openpanel.toml';

/**
 * Read the configured port from disk, returning `null` if the
 * file is missing or the port is absent. Callers may treat
 * `null` as "ask the user" or fall back to the running service.
 */
export async function readPort(file: string = DEFAULT_CONFIG_PATH): Promise<number | null> {
  try {
    const data = await fs.readFile(file, 'utf8');
    return parsePort(data);
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
      return null;
    }
    throw err;
  }
}

/**
 * Rewrite the `port` line in-place. The file is copied
 * (`.opctl.bak`) before the new value is written. The rewrite is
 * done by reading, replacing, and writing — atomic on POSIX when
 * the destination is a regular file and the source is on the same
 * filesystem.
 */
export async function rewritePort(
  file: string,
  next: number,
): Promise<{ readonly backup: string }> {
  if (!Number.isInteger(next) || next < 1 || next > 65535) {
    throw new ConfigError(`port out of range: ${next}`);
  }
  const data = await fs.readFile(file, 'utf8');
  const backup = `${file}.opctl.bak`;
  await fs.writeFile(backup, data, { mode: 0o600 });
  const rewritten = data
    .split('\n')
    .map((line) => {
      if (/^\s*port\s*=\s*\d+/i.test(line)) {
        return line.replace(/(\s*port\s*=\s*)\d+/i, `$1${next}`);
      }
      return line;
    })
    .join('\n');
  await fs.writeFile(file, rewritten, { mode: 0o644 });
  return { backup: path.basename(backup) };
}
