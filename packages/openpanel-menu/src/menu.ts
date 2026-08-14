/**
 * Dispatcher table and `runMenu` loop.
 *
 * The dispatcher:
 *  - reads one choice from the operator
 *  - trims full-width + ASCII whitespace
 *  - dispatches to the handler registered for the numeric id
 *  - re-prompts on bad input (never crashes)
 *  - exits only when the operator types `0`
 *
 * The production `runMenu` creates a `PromptSession` and shares
 * it across all prompts so a single `readline` interface reads
 * the whole session. Tests inject their own `CommandContext`
 * (via `makeTestContext`) and do not need a session.
 */

import process from 'node:process';
import {
  run as execRun,
  makeShellExec,
  type ExecLike,
  type Logger,
  makeLogger,
  makeRecordingLogger,
  consumeStdinEnded,
  PromptSession,
} from './lib/index.js';
import { MENU_ITEMS, renderHeader } from './menu-items.js';
import { EXIT_ID, type CommandContext, type CommandOutcome, type Handler, type PromptLineOptions } from './menu-data.js';

export { MENU_ITEMS, renderHeader, EXIT_ID };
export type { CommandContext, CommandOutcome, Handler };

/**
 * Read a single choice from the operator. When the input is
 * empty AND stdin has reached EOF (e.g. the operator hit
 * Ctrl-D, or the menu was launched non-interactively), the
 * session exits gracefully instead of looping.
 */
export async function readChoice(
  session: PromptSession,
  log: Logger,
): Promise<number | null> {
  const answer = await session.question('请输入命令编号', undefined, log);
  const trimmed = answer.replace(/[\s\u3000]+/g, '');
  if (trimmed === '') {
    if (consumeStdinEnded()) {
      log.info('再见.');
      return EXIT_ID;
    }
    log.error('无效的命令编号,请重新输入.');
    return null;
  }
  if (!/^\d+$/.test(trimmed)) {
    log.error('无效的命令编号,请重新输入.');
    return null;
  }
  const value = Number.parseInt(trimmed, 10);
  if (value === EXIT_ID) {
    return EXIT_ID;
  }
  if (!MENU_ITEMS.some((item) => item.id === value)) {
    log.error('无效的命令编号,请重新输入.');
    return null;
  }
  return value;
}

/** Read a single line for arbitrary prompting; used by handlers. */
export async function promptLineForCtx(
  ctx: CommandContext,
  message: string,
  opts?: PromptLineOptions,
): Promise<string> {
  return ctx.promptLine(message, opts);
}

/** Build a real `CommandContext` against the live process, sharing one readline session. */
export function makeLiveContext(
  version: string,
  session: PromptSession,
  exec: ExecLike = makeShellExec(),
): CommandContext {
  const log = makeLogger(process.stdout, process.stderr);
  return {
    promptLine: (msg, opts) => session.question(msg, opts ?? {}, log),
    promptHidden: (msg) => session.hidden(msg, log),
    promptConfirm: (msg, defaultYes) => session.confirm(msg, defaultYes ?? false, log),
    log,
    exec,
    version,
  };
}

/**
 * Test context builder. Defaults to a recording logger, a stub
 * exec that records every argv, and prompt stubs that consume
 * pre-configured answers in order.
 */
export interface TestContextOverrides {
  readonly answers?: ReadonlyArray<string>;
  readonly confirms?: ReadonlyArray<boolean>;
  readonly exec?: ExecLike;
  readonly version?: string;
}

export interface TestContextResult {
  readonly ctx: CommandContext;
  readonly log: ReturnType<typeof makeRecordingLogger>;
  readonly execCalls: ReadonlyArray<ReadonlyArray<string>>;
  readonly prompts: ReadonlyArray<{ readonly message: string; readonly defaultValue?: string }>;
  readonly confirms: ReadonlyArray<{ readonly message: string; readonly defaultYes: boolean | undefined }>;
  readonly hidden: ReadonlyArray<string>;
}

export function makeTestContext(overrides: TestContextOverrides = {}): TestContextResult {
  const answers = overrides.answers ? [...overrides.answers] : [];
  const confirms = overrides.confirms ? [...overrides.confirms] : [];
  const version = overrides.version ?? '0.1.0';

  const log = makeRecordingLogger();

  const execCalls: Array<ReadonlyArray<string>> = [];
  const exec: ExecLike = overrides.exec ?? {
    run: async (argv) => {
      execCalls.push([...argv]);
      return {
        ok: true,
        code: 0,
        signal: null,
        stdout: '',
        stderr: '',
        argv: [...argv],
      };
    },
  };

  const prompts: Array<{ message: string; defaultValue?: string }> = [];
  const confirmCalls: Array<{ message: string; defaultYes: boolean | undefined }> = [];
  const hidden: Array<string> = [];

  let answerIdx = 0;
  let confirmIdx = 0;

  const ctx: CommandContext = {
    promptLine: async (message, opts) => {
      prompts.push(opts?.defaultValue !== undefined ? { message, defaultValue: opts.defaultValue } : { message });
      const next = answers[answerIdx];
      answerIdx += 1;
      return next ?? '';
    },
    promptHidden: async (message) => {
      hidden.push(message);
      const next = answers[answerIdx];
      answerIdx += 1;
      return next ?? '';
    },
    promptConfirm: async (message, defaultYes) => {
      confirmCalls.push({ message, defaultYes });
      const next = confirms[confirmIdx];
      confirmIdx += 1;
      return next ?? (defaultYes ?? false);
    },
    log,
    exec,
    version,
  };

  return {
    ctx,
    log,
    execCalls,
    prompts,
    confirms: confirmCalls,
    hidden,
  };
}

/** Drive the menu loop with the given context (used by tests; production uses `runLiveMenu`). */
export async function runMenu(ctx: CommandContext, handlers: ReadonlyMap<number, Handler>, session?: PromptSession): Promise<void> {
  // Tests pass a context whose prompts are already stubbed; we still
  // need a session for `readChoice`. The session is created lazily
  // and is only used here. Production callers should use
  // `runLiveMenu` which creates the session once and shares it.
  const s = session ?? new PromptSession();
  try {
    while (true) {
      ctx.log.info(renderHeader(ctx.version));
      const choice = await readChoice(s, ctx.log);
      if (choice === null) {
        continue;
      }
      if (choice === EXIT_ID) {
        ctx.log.info('再见.');
        return;
      }
      const handler = handlers.get(choice);
      if (handler === undefined) {
        ctx.log.error('无效的命令编号,请重新输入.');
        continue;
      }
      const outcome: CommandOutcome = await handler(ctx);
      if (outcome.kind === 'exit') {
        return;
      }
    }
  } finally {
    s.close();
  }
}

/**
 * Production entry point. Creates a single `PromptSession` and a
 * live context, then runs the dispatcher until the operator
 * types `0` (or stdin reaches EOF).
 */
export async function runLiveMenu(
  version: string,
  handlers: ReadonlyMap<number, Handler>,
  exec: ExecLike = makeShellExec(),
): Promise<void> {
  const session = new PromptSession();
  const ctx = makeLiveContext(version, session, exec);
  try {
    while (true) {
      ctx.log.info(renderHeader(ctx.version));
      const choice = await readChoice(session, ctx.log);
      if (choice === null) {
        continue;
      }
      if (choice === EXIT_ID) {
        ctx.log.info('再见.');
        return;
      }
      const handler = handlers.get(choice);
      if (handler === undefined) {
        ctx.log.error('无效的命令编号,请重新输入.');
        continue;
      }
      const outcome: CommandOutcome = await handler(ctx);
      if (outcome.kind === 'exit') {
        return;
      }
    }
  } finally {
    session.close();
  }
}

// Re-exec helper for production.
export { execRun };
