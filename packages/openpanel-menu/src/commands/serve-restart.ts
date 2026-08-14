/**
 * `serve-restart` — restart the panel via systemd.
 *
 * Pre-checks the unit file before issuing the restart; on success
 * sleeps 1 second and probes `is-active`.
 */

import type { CommandContext, CommandOutcome } from '../menu-data.js';

export async function run(ctx: CommandContext): Promise<CommandOutcome> {
  const cat = await ctx.exec.run(['systemctl', 'cat', 'openpanel.service']);
  if (!cat.ok) {
    ctx.log.warn('未发现 openpanel.service 单元文件.');
    return { kind: 'continue' };
  }
  const restart = await ctx.exec.run(['systemctl', 'restart', 'openpanel.service']);
  if (!restart.ok) {
    ctx.log.error(`重启失败: ${restart.stderr.trim() || '未知错误'}`);
    return { kind: 'continue' };
  }
  await new Promise((resolve) => setTimeout(resolve, 1000));
  const probe = await ctx.exec.run(['systemctl', 'is-active', 'openpanel.service']);
  if (!probe.ok || probe.stdout.trim() !== 'active') {
    ctx.log.warn(`重启后状态异常: ${probe.stdout.trim() || 'inactive'}`);
    return { kind: 'continue' };
  }
  ctx.log.ok('openpanel 服务已重启.');
  return { kind: 'continue' };
}
