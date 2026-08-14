/** Public re-exports for the lib package. */
export type { ExecResult, ExecLike } from './exec.js';
export { ExecError, makeShellExec, run } from './exec.js';
export type { Logger } from './log.js';
export { makeLogger, makeRecordingLogger, stripAnsi } from './log.js';
export type { PromptLineOptions, ReadlineFactory } from './prompt.js';
export {
  promptLine,
  promptConfirm,
  promptHidden,
  setReadlineFactory,
  resetReadlineFactory,
  consumeStdinEnded,
  PromptSession,
} from './prompt.js';
export type { SttyResult } from './stty.js';
export { hasTty, sttyEchoOff, sttyEchoOn } from './stty.js';
export type { ConfigError as ConfigErrorType } from './config.js';
export { ConfigError, DEFAULT_CONFIG_PATH, parsePort, readPort, rewritePort } from './config.js';
