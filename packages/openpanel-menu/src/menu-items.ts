/**
 * The static menu table. The order MUST be preserved across
 * versions; new rows may be appended after id 7 but never
 * replace existing rows.
 */

import type { MenuItem } from './menu-data.js';

export const MENU_ITEMS: ReadonlyArray<MenuItem> = [
  { id: 1, title: '重启面板', description: 'restart panel' },
  { id: 2, title: '停止面板', description: 'stop panel' },
  { id: 3, title: '启动面板', description: 'start panel' },
  { id: 4, title: '修改面板端口', description: 'change panel port' },
  { id: 5, title: '修改管理员密码', description: 'change admin password' },
  { id: 6, title: '查看面板信息', description: 'show info' },
  { id: 7, title: '升级面板', description: 'upgrade' },
];

/** Render the two-column banner. */
export function renderHeader(version: string): string {
  const lines: string[] = [];
  lines.push(`=========== OpenPanel 控制菜单 v${version} ===========`);
  lines.push('(1) 重启面板         (2) 停止面板         (3) 启动面板');
  lines.push('(4) 修改面板端口     (5) 修改管理员密码   (6) 查看面板信息');
  lines.push('(7) 升级面板         (0) 退出');
  lines.push('================================================');
  lines.push('请输入命令编号:');
  return lines.join('\n');
}
