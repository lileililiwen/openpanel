/**
 * Entry point for `opctl` and `openpanel-menu`.
 *
 * Builds the live context, wires the seven handlers, and runs
 * the dispatcher until the operator types `0`.
 */

import process from 'node:process';
import { runLiveMenu } from './menu.js';
import * as commands from './commands/index.js';
import type { Handler } from './menu-data.js';

const VERSION = '0.1.0';

const handlers: ReadonlyMap<number, Handler> = new Map<number, Handler>([
  [1, commands.serveRestart],
  [2, commands.serveStop],
  [3, commands.serveStart],
  [4, commands.changePort],
  [5, commands.changePassword],
  [6, commands.showInfo],
  [7, commands.upgrade],
]);

export async function main(argv: ReadonlyArray<string> = process.argv.slice(2)): Promise<number> {
  if (argv.includes('--version') || argv.includes('-V')) {
    process.stdout.write(`opctl ${VERSION}\n`);
    return 0;
  }
  if (argv.includes('--help') || argv.includes('-h')) {
    process.stdout.write(`Usage: opctl [--version] [--help]\n`);
    process.stdout.write(`Interactive menu: run without arguments.\n`);
    return 0;
  }
  await runLiveMenu(VERSION, handlers);
  return 0;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().then(
    (code) => {
      process.exit(code);
    },
    (err) => {
      process.stderr.write(`[ERROR] ${err instanceof Error ? err.message : String(err)}\n`);
      process.exit(1);
    },
  );
}
