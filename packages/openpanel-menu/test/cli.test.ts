import { test, describe } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '..', '..', '..');
const indexTs = path.join(repoRoot, 'packages', 'openpanel-menu', 'src', 'index.ts');

/** Spawn `tsx` with the menu's entry and pipe stdin. */
function spawnMenu(stdinText: string): Promise<{ code: number | null; stdout: string; stderr: string; combined: string }> {
  return new Promise((resolve, reject) => {
    const child = spawn(
      'npx',
      ['--no-install', 'tsx', indexTs],
      { shell: false, stdio: ['pipe', 'pipe', 'pipe'] },
    );
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (b) => {
      stdout += b.toString();
    });
    child.stderr.on('data', (b) => {
      stderr += b.toString();
    });
    child.on('error', reject);
    child.on('close', (code) => {
      resolve({ code, stdout, stderr, combined: stdout + stderr });
    });
    child.stdin.write(stdinText);
    child.stdin.end();
  });
}

describe('opctl CLI E2E', () => {
  test('prints the banner with every menu row', async () => {
    const { stdout, code } = await spawnMenu('0\n');
    assert.equal(code, 0);
    assert.match(stdout, /OpenPanel 控制菜单/);
    assert.match(stdout, /\(1\) 重启面板/);
    assert.match(stdout, /\(0\) 退出/);
  });

  test('re-prompts on non-numeric input and exits on 0', async () => {
    const { combined, code } = await spawnMenu('abc\n0\n');
    assert.equal(code, 0);
    assert.ok(combined.includes('无效的命令编号'));
    assert.ok(combined.includes('再见.'));
  });

  test('"4" for change-port does not panic with no config', async () => {
    const { code } = await spawnMenu('4\n0\n');
    assert.equal(code, 0);
  });
});
