import { test, describe } from 'node:test';
import assert from 'node:assert/strict';
import {
  MENU_ITEMS,
  renderHeader,
  EXIT_ID,
  readChoice,
  runMenu,
  makeTestContext,
} from '../src/menu.js';
import { PromptSession, setReadlineFactory, resetReadlineFactory } from '../src/lib/prompt.js';
import type { Interface } from 'node:readline/promises';
import type { CommandOutcome } from '../src/menu-data.js';

function makeStubSession(answers: ReadonlyArray<string>): PromptSession {
  let i = 0;
  const stub: Interface = {
    question: async () => {
      const next = answers[i] ?? '';
      i += 1;
      return next;
    },
    close: () => undefined,
  } as unknown as Interface;
  return new PromptSession(stub);
}

describe('MENU_ITEMS', () => {
  test('excludes the reserved exit id', () => {
    for (const item of MENU_ITEMS) {
      assert.notEqual(item.id, EXIT_ID);
    }
  });

  test('contains ids 1..7 in order', () => {
    const ids = MENU_ITEMS.map((i) => i.id);
    assert.deepEqual(ids, [1, 2, 3, 4, 5, 6, 7]);
  });

  test('every id has a non-empty title and description', () => {
    for (const item of MENU_ITEMS) {
      assert.ok(item.title.length > 0, `id ${item.id} title is empty`);
      assert.ok(item.description.length > 0, `id ${item.id} description is empty`);
    }
  });
});

describe('renderHeader', () => {
  test('contains every menu row', () => {
    const out = renderHeader('0.1.0');
    assert.match(out, /\(1\) 重启面板/);
    assert.match(out, /\(2\) 停止面板/);
    assert.match(out, /\(3\) 启动面板/);
    assert.match(out, /\(4\) 修改面板端口/);
    assert.match(out, /\(5\) 修改管理员密码/);
    assert.match(out, /\(6\) 查看面板信息/);
    assert.match(out, /\(7\) 升级面板/);
    assert.match(out, /\(0\) 退出/);
  });

  test('version appears in the title', () => {
    const out = renderHeader('0.2.0');
    assert.match(out, /v0\.2\.0/);
  });
});

describe('readChoice', () => {
  test('rejects non-numeric input', async () => {
    const { ctx, log } = makeTestContext();
    const session = makeStubSession(['abc']);
    const result = await readChoice(session, ctx.log);
    session.close();
    assert.equal(result, null);
    assert.ok(log.lines.some((l) => l.includes('无效的命令编号')));
  });

  test('rejects out-of-range numeric input', async () => {
    const { ctx, log } = makeTestContext();
    const session = makeStubSession(['99']);
    const result = await readChoice(session, ctx.log);
    session.close();
    assert.equal(result, null);
    assert.ok(log.lines.some((l) => l.includes('无效的命令编号')));
  });

  test('rejects empty input', async () => {
    const { ctx, log } = makeTestContext();
    const session = makeStubSession(['']);
    const result = await readChoice(session, ctx.log);
    session.close();
    assert.equal(result, null);
  });

  test('treats whitespace + full-width as digit', async () => {
    const { ctx } = makeTestContext();
    const session = makeStubSession(['  \u3000 1 \u3000  ']);
    const result = await readChoice(session, ctx.log);
    session.close();
    assert.equal(result, 1);
  });

  test('returns the in-range id', async () => {
    const { ctx } = makeTestContext();
    const session = makeStubSession(['3']);
    const result = await readChoice(session, ctx.log);
    session.close();
    assert.equal(result, 3);
  });

  test('returns EXIT_ID for "0"', async () => {
    const { ctx } = makeTestContext();
    const session = makeStubSession(['0']);
    const result = await readChoice(session, ctx.log);
    session.close();
    assert.equal(result, 0);
  });
});

describe('runMenu (test context)', () => {
  test('dispatches to the registered handler and continues', async () => {
    const { ctx, prompts } = makeTestContext();
    const session = makeStubSession(['1', '0']);
    let calls = 0;
    const handlers = new Map<number, (ctx: typeof ctx) => Promise<CommandOutcome>>([
      [
        1,
        async () => {
          calls += 1;
          return { kind: 'continue' };
        },
      ],
    ]);
    await runMenu(ctx, handlers, session);
    assert.equal(calls, 1);
  });

  test('re-prompts on bad input without crashing', async () => {
    const { ctx } = makeTestContext();
    const session = makeStubSession(['abc', '0']);
    const handlers = new Map<number, (ctx: typeof ctx) => Promise<CommandOutcome>>();
    await runMenu(ctx, handlers, session);
  });

  test('exits when handler returns exit', async () => {
    const { ctx } = makeTestContext();
    const session = makeStubSession(['1']);
    const handlers = new Map<number, (ctx: typeof ctx) => Promise<CommandOutcome>>([
      [
        1,
        async () => {
          return { kind: 'exit' };
        },
      ],
    ]);
    await runMenu(ctx, handlers, session);
  });
});
