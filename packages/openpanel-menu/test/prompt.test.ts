import { test, describe, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import {
  promptLine,
  promptConfirm,
  promptHidden,
  setReadlineFactory,
  resetReadlineFactory,
} from '../src/lib/prompt.js';
import { makeRecordingLogger } from '../src/lib/log.js';
import type { Interface } from 'node:readline/promises';

function makeRl(answers: ReadonlyArray<string>): Interface {
  let i = 0;
  return {
    question: async () => {
      const next = answers[i] ?? '';
      i += 1;
      return next;
    },
    close: () => undefined,
  } as unknown as Interface;
}

afterEach(() => {
  resetReadlineFactory();
});

describe('promptLine', () => {
  test('returns trimmed input', async () => {
    setReadlineFactory(() => makeRl(['  hello  ']));
    const log = makeRecordingLogger();
    const result = await promptLine('msg', {}, log);
    assert.equal(result, 'hello');
  });

  test('applies default on empty input', async () => {
    setReadlineFactory(() => makeRl(['']));
    const log = makeRecordingLogger();
    const result = await promptLine('msg', { defaultValue: 'admin' }, log);
    assert.equal(result, 'admin');
  });

  test('returns empty when no default', async () => {
    setReadlineFactory(() => makeRl(['']));
    const log = makeRecordingLogger();
    const result = await promptLine('msg', {}, log);
    assert.equal(result, '');
  });
});

describe('promptConfirm', () => {
  test('y → true', async () => {
    setReadlineFactory(() => makeRl(['y']));
    const log = makeRecordingLogger();
    const result = await promptConfirm('go?', false, log);
    assert.equal(result, true);
  });

  test('yes → true', async () => {
    setReadlineFactory(() => makeRl(['yes']));
    const log = makeRecordingLogger();
    const result = await promptConfirm('go?', false, log);
    assert.equal(result, true);
  });

  test('n → false', async () => {
    setReadlineFactory(() => makeRl(['n']));
    const log = makeRecordingLogger();
    const result = await promptConfirm('go?', true, log);
    assert.equal(result, false);
  });

  test('empty input follows default', async () => {
    setReadlineFactory(() => makeRl(['']));
    const log = makeRecordingLogger();
    const result = await promptConfirm('go?', true, log);
    assert.equal(result, true);
  });

  test('unknown value follows default', async () => {
    setReadlineFactory(() => makeRl(['maybe']));
    const log = makeRecordingLogger();
    const result = await promptConfirm('go?', false, log);
    assert.equal(result, false);
  });
});

describe('promptHidden (non-TTY fallback)', () => {
  test('returns the typed value', async () => {
    setReadlineFactory(() => makeRl(['swordfish']));
    const log = makeRecordingLogger();
    const result = await promptHidden('password', log);
    assert.equal(result, 'swordfish');
  });

  test('emits a warning when not a TTY', async () => {
    setReadlineFactory(() => makeRl(['swordfish']));
    const log = makeRecordingLogger();
    await promptHidden('password', log);
    assert.ok(log.lines.some((l) => l.includes('警告')));
  });
});
