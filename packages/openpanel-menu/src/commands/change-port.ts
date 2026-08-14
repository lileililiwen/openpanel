/**
 * `change-port` — change the panel's listen port.
 *
 * Reads the current port from `/etc/openpanel/openpanel.toml`,
 * validates the new port (1..65535 and not already bound per
 * `ss -ltn`), backs the file up as `<file>.opctl.bak`, rewrites
 * the `port = NNNN` line, and (on confirmation) restarts the unit.
 */

import { readPort, rewritePort, DEFAULT_CONFIG_PATH } from '../lib/config.js';
import type { CommandContext, CommandOutcome } from '../menu-data.js';

const MIN_PORT = 1024;
const MAX_PORT = 65535;

async function isPortBound(port: number, exec: CommandContext['exec']): Promise<boolean> {
  const ss = await exec.run(['ss', '-ltn']);
  if (!ss.ok) {
    return false;
  }
  const lines = ss.stdout.split('\n');
  return lines.some((line) => {
    if (!line.includes('LISTEN')) {
      return false;
    }
    const m = /[:\.](\d+)\s*$/.exec(line);
    if (!m) {
      return false;
    }
    return Number.parseInt(m[1] ?? '0', 10) === port;
  });
}

export async function run(ctx: CommandContext): Promise<CommandOutcome> {
  const current = await readPort(DEFAULT_CONFIG_PATH);
  if (current === null) {
    ctx.log.warn('无法读取当前端口: 配置文件缺失.');
    return { kind: 'continue' };
  }
  const next = await ctx.promptLine('请输入新端口', { defaultValue: String(current) });
  const parsed = Number.parseInt(next, 10);
  if (!Number.isInteger(parsed) || parsed < MIN_PORT || parsed > MAX_PORT) {
    ctx.log.error(`端口号必须是 ${MIN_PORT}-${MAX_PORT} 之间的整数.`);
    return { kind: 'continue' };
  }
  if (parsed === current) {
    ctx.log.info('新端口与当前相同 — 无需修改.');
    return { kind: 'continue' };
  }
  if (await isPortBound(parsed, ctx.exec)) {
    ctx.log.error(`端口 ${parsed} 已被其他进程占用.`);
    return { kind: 'continue' };
  }
  const { backup } = await rewritePort(DEFAULT_CONFIG_PATH, parsed);
  ctx.log.info(`配置已备份为 ${backup}.`);
  const restart = await ctx.promptConfirm(`新端口 ${parsed} 已写入. 立即重启服务?`, true);
  if (!restart) {
    ctx.log.info('已写入配置,稍后请手动重启服务以应用新端口.');
    return { kind: 'continue' };
  }
  const result = await ctx.exec.run(['systemctl', 'restart', 'openpanel.service']);
  if (!result.ok) {
    ctx.log.error(
      `重启失败: ${result.stderr.trim() || '未知错误'}. ` +
        `如需回滚: cp ${DEFAULT_CONFIG_PATH}.opctl.bak ${DEFAULT_CONFIG_PATH} && systemctl restart openpanel.service`,
    );
    return { kind: 'continue' };
  }
  ctx.log.ok(`端口已更新为 ${parsed},服务已重启.`);
  return { kind: 'continue' };
}
