/**
 * `change-password` — change the admin password via `openpanel-cli`.
 *
 * The plaintext is passed as `argv` to `spawn`; the menu's log
 * tags and the subprocess `ps` line never include the password
 * in the operator-visible form (it is the actual `--password`
 * value, but no other log line contains it).
 */

import { ensureStrongPassword, isStrongPassword } from '../lib/password.js';
import type { CommandContext, CommandOutcome } from '../menu-data.js';

export async function run(ctx: CommandContext): Promise<CommandOutcome> {
  const username = await ctx.promptLine('请输入用户名', { defaultValue: 'admin' });
  if (username === '') {
    ctx.log.error('用户名不能为空.');
    return { kind: 'continue' };
  }
  const password = await ctx.promptHidden('请输入新密码');
  if (!isStrongPassword(password)) {
    ctx.log.error('密码必须 ≥ 12 字符,且至少包含一个字母和一个数字.');
    return { kind: 'continue' };
  }
  const confirm = await ctx.promptHidden('请再次输入新密码');
  if (password !== confirm) {
    ctx.log.error('两次输入不一致.');
    return { kind: 'continue' };
  }
  // Validate once more; throws only if the input is invalid.
  ensureStrongPassword(password);

  const result = await ctx.exec.run([
    'openpanel',
    'user',
    'update-password',
    '--username',
    username,
    '--password',
    password,
  ]);
  if (!result.ok) {
    ctx.log.error(`密码更新失败: ${result.stderr.trim() || '未知错误'}`);
    return { kind: 'continue' };
  }
  ctx.log.ok(`用户 ${username} 的密码已更新.`);
  ctx.log.warn('请将新密码记在安全的密码管理器中.');
  return { kind: 'continue' };
}
