/**
 * `serve-stop` — stop the panel via systemd.
 *
 * Requires a confirmation; on cancel the unit is untouched.
 */

import type { CommandContext, CommandOutcome } from '../menu-data.js';

export async function run(ctx: CommandContext): Promise<CommandOutcome> {
  const confirmed = await ctx.promptConfirm('确定要停止 openpanel 服务吗?', false);
  if (!confirmed) {
    ctx.log.info('已取消.');
    return { kind: 'continue' };
  }
  const result = await ctx.exec.run(['systemctl', 'stop', 'openpanel.service']);
  if (!result.ok) {
    ctx.log.error(`停止失败: ${result.stderr.trim() || '未知错误'}`);
    return { kind: 'continue' };
  }
  ctx.log.ok('openpanel 服务已停止.');
  return { kind: 'continue' };
}
