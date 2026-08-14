/**
 * Typed domain for the OpenPanel control menu.
 *
 * Every public type the menu exposes lives here so the dispatcher,
 * the command modules, and the tests all share one shape. All
 * exported types are `readonly` so menu tables and handler maps
 * cannot drift at runtime.
 */

import type { ExecResult, ExecLike } from './lib/exec.js';
import type { Logger } from './lib/log.js';

/**
 * Numeric id reserved for the "exit" row. The id `0` is NOT a row
 * in `MENU_ITEMS`; it lives in `EXIT_ID` and is the only way the
 * dispatcher terminates the session.
 */
export const EXIT_ID = 0 as const;

/**
 * A single menu row. The id is the operator's choice; the title is
 * the Chinese label; the description is the English description
 * used by tests and the non-TTY banner.
 */
export interface MenuItem {
  readonly id: number;
  readonly title: string;
  readonly description: string;
}

/**
 * The outcome a handler returns. `continue` re-prompts the menu;
 * `exit` ends the session. No other shape is allowed.
 */
export type CommandOutcome =
  | { readonly kind: 'continue' }
  | { readonly kind: 'exit' };

/**
 * The single context every command module receives. `runMenu`
 * builds a real one; tests build a stub via `makeTestContext`.
 */
export interface CommandContext {
  readonly promptLine: (msg: string, opts?: PromptLineOptions) => Promise<string>;
  readonly promptHidden: (msg: string) => Promise<string>;
  readonly promptConfirm: (msg: string, defaultYes?: boolean) => Promise<boolean>;
  readonly log: Logger;
  readonly exec: ExecLike;
  readonly version: string;
}

/** Options for `promptLine`. */
export interface PromptLineOptions {
  readonly defaultValue?: string;
}

/**
 * The full handler shape a command module must export.
 *
 * `export async function run(ctx: CommandContext): Promise<CommandOutcome>`
 */
export type Handler = (ctx: CommandContext) => Promise<CommandOutcome>;

/** Re-exported so command modules only need one import. */
export type { ExecResult, ExecLike, Logger };
