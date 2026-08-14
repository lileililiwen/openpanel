import { test, describe } from 'node:test';
import assert from 'node:assert/strict';
import { makeTestContext } from '../src/menu.js';
import { serveStart } from '../src/commands/index.js';

describe('serve-start', () => {
  test('short-circuits when is-active returns active', async () => {
    const { ctx, execCalls, log } = makeTestContext();
    const recorded: Array<{ ok: boolean; stdout: string; code: number | null }> = [];
    const exec = {
      run: async (argv: ReadonlyArray<string>) => {
        execCalls.push([...argv]);
        if (argv[0] === 'systemctl' && argv[1] === 'is-active') {
          const r = { ok: true, code: 0, signal: null, stdout: 'active\n', stderr: '', argv: [...argv] };
          recorded.push({ ok: r.ok, stdout: r.stdout, code: r.code });
          return r;
        }
        return { ok: true, code: 0, signal: null, stdout: '', stderr: '', argv: [...argv] };
      },
    };
    const wrapped = { ...ctx, exec };
    const outcome = await serveStart(wrapped);
    assert.equal(outcome.kind, 'continue');
    assert.ok(log.lines.some((l) => l.includes('已在运行')));
    assert.equal(execCalls.length, 1);
  });

  test('starts when inactive', async () => {
    const { ctx, execCalls, log } = makeTestContext();
    const exec = {
      run: async (argv: ReadonlyArray<string>) => {
        execCalls.push([...argv]);
        if (argv[1] === 'is-active') {
          return { ok: true, code: 0, signal: null, stdout: 'inactive\n', stderr: '', argv: [...argv] };
        }
        return { ok: true, code: 0, signal: null, stdout: '', stderr: '', argv: [...argv] };
      },
    };
    const wrapped = { ...ctx, exec };
    await serveStart(wrapped);
    assert.equal(execCalls.length, 2);
    assert.deepEqual([...execCalls[1]!], ['systemctl', 'start', 'openpanel.service']);
    assert.ok(log.lines.some((l) => l.includes('已启动')));
  });

  test('warns when unit missing', async () => {
    const { ctx, execCalls, log } = makeTestContext();
    const exec = {
      run: async (argv: ReadonlyArray<string>) => {
        execCalls.push([...argv]);
        return { ok: false, code: 5, signal: null, stdout: '', stderr: 'Failed to ... not-found\n', argv: [...argv] };
      },
    };
    const wrapped = { ...ctx, exec };
    await serveStart(wrapped);
    assert.ok(log.lines.some((l) => l.includes('未发现')));
  });
});
