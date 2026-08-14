/**
 * `show-info` — read-only display of version / port / state.
 */

import { readPort, DEFAULT_CONFIG_PATH } from '../lib/config.js';
import type { CommandContext, CommandOutcome } from '../menu-data.js';

export async function run(ctx: CommandContext): Promise<CommandOutcome> {
  const versionResult = await ctx.exec.run(['openpanel', '--version']);
  const version = versionResult.ok ? versionResult.stdout.trim() : 'unknown';
  const port = await readPort(DEFAULT_CONFIG_PATH);
  const stateResult = await ctx.exec.run(['systemctl', 'is-active', 'openpanel.service']);
  const state = stateResult.ok ? stateResult.stdout.trim() : 'inactive/dead';
  ctx.log.info(`版本: ${version} / 监听端口: ${port ?? 'unknown'} / systemd 状态: ${state}`);
  return { kind: 'continue' };
}
