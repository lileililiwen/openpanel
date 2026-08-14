import { test, describe } from 'node:test';
import assert from 'node:assert/strict';
import { run, ExecError } from '../src/lib/exec.js';

describe('run (real spawns)', () => {
  test('node --version exits 0 and emits v<digit>', async () => {
    const result = await run(['node', '--version']);
    assert.equal(result.ok, true);
    assert.equal(result.code, 0);
    assert.match(result.stdout, /^v\d/);
  });

  test('non-zero exit is reported via ok=false (no throw)', async () => {
    const result = await run(['node', '-e', 'process.exit(7)']);
    assert.equal(result.ok, false);
    assert.equal(result.code, 7);
  });

  test('missing binary throws ExecError', async () => {
    await assert.rejects(
      () => run(['this-binary-does-not-exist-9999', '--help']),
      (err: unknown) => err instanceof ExecError,
    );
  });

  test('captures stderr separately from stdout', async () => {
    const result = await run(['node', '-e', 'process.stderr.write("oops"); process.exit(0);']);
    assert.equal(result.stdout.trim(), '');
    assert.equal(result.stderr.trim(), 'oops');
  });
});
