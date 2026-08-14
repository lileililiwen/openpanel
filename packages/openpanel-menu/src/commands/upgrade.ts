/**
 * `upgrade` — install a newer `openpanel-cli` and/or `openpanel-menu`,
 * then restart the service.
 *
 * Each sub-step fails independently; the service restart is the
 * very last step.
 */

import type { CommandContext, CommandOutcome } from '../menu-data.js';

async function upgradeCli(ctx: CommandContext): Promise<boolean> {
  const result = await ctx.exec.run(['cargo', 'install', 'openpanel-cli', '--locked', '--force']);
  if (!result.ok) {
    ctx.log.error(`cli 升级失败: ${result.stderr.trim() || '未知错误'}`);
    return false;
  }
  ctx.log.ok('cli 已升级.');
  return true;
}

async function upgradeMenu(ctx: CommandContext): Promise<boolean> {
  const result = await ctx.exec.run(['npm', 'install', '-g', 'openpanel-menu@latest']);
  if (!result.ok) {
    ctx.log.error(`menu 升级失败: ${result.stderr.trim() || '未知错误'}`);
    return false;
  }
  ctx.log.ok('menu 已升级.');
  return true;
}

export async function run(ctx: CommandContext): Promise<CommandOutcome> {
  const choice = await ctx.promptLine('升级目标 (1=cli, 2=menu, 3=both)', { defaultValue: '3' });
  const target = Number.parseInt(choice, 10);
  if (![1, 2, 3].includes(target)) {
    ctx.log.error('请输入 1、2 或 3.');
    return { kind: 'continue' };
  }
  const confirmed = await ctx.promptConfirm('确认开始升级?', false);
  if (!confirmed) {
    ctx.log.info('已取消.');
    return { kind: 'continue' };
  }

  let cliOk = true;
  let menuOk = true;
  if (target === 1 || target === 3) {
    cliOk = await upgradeCli(ctx);
  }
  if (target === 2 || target === 3) {
    menuOk = await upgradeMenu(ctx);
  }
  if (!cliOk || !menuOk) {
    ctx.log.warn('部分步骤失败,请检查上方错误信息.');
    return { kind: 'continue' };
  }
  const restart = await ctx.exec.run(['systemctl', 'restart', 'openpanel.service']);
  if (!restart.ok) {
    ctx.log.error(`重启失败: ${restart.stderr.trim() || '未知错误'}`);
    return { kind: 'continue' };
  }
  ctx.log.ok('升级完成,服务已重启.');
  return { kind: 'continue' };
}
