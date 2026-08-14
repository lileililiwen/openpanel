/**
 * `serve-start` — start the panel via systemd.
 *
 * Idempotent: pre-checks `is-active` and short-circuits if the
 * service is already running.
 */

import type { CommandContext, CommandOutcome } from '../menu-data.js';

export async function run(ctx: CommandContext): Promise<CommandOutcome> {
  const probe = await ctx.exec.run(['systemctl', 'is-active', 'openpanel.service']);
  if (probe.ok && probe.stdout.trim() === 'active') {
    ctx.log.info('openpanel 服务已在运行 — 无需重复启动.');
    return { kind: 'continue' };
  }
  if (!probe.ok && probe.stderr.trim() !== '' && !/not-found/i.test(probe.stderr)) {
    ctx.log.warn(`无法查询服务状态: ${probe.stderr.trim()}`);
  }
  const result = await ctx.exec.run(['systemctl', 'start', 'openpanel.service']);
  if (!result.ok) {
    if (/not-found/i.test(result.stderr) || result.code === 5) {
      ctx.log.warn('未发现 openpanel.service 单元文件.');
      return { kind: 'continue' };
    }
    ctx.log.error(`启动失败: ${result.stderr.trim() || '未知错误'}`);
    return { kind: 'continue' };
  }
  ctx.log.ok('openpanel 服务已启动.');
  return { kind: 'continue' };
}
