import { test, describe } from 'node:test';
import assert from 'node:assert/strict';
import { makeLogger, makeRecordingLogger, stripAnsi } from '../src/lib/log.js';

describe('makeRecordingLogger', () => {
  test('every method records a line with the right tag', () => {
    const log = makeRecordingLogger();
    log.info('one');
    log.ok('two');
    log.warn('three');
    log.error('four');
    log.raw('five');
    assert.deepEqual(log.lines, [
      '[INFO] one',
      '[OK] two',
      '[WARN] three',
      '[ERROR] four',
      '[RAW] five',
    ]);
  });
});

describe('stripAnsi', () => {
  test('removes color escape sequences', () => {
    assert.equal(stripAnsi('\x1b[31mred\x1b[0m'), 'red');
  });
});

describe('makeLogger', () => {
  test('writes lines to the provided stream', () => {
    let captured: string[] = [];
    const stream: NodeJS.WriteStream = {
      isTTY: false,
      write: (s: string) => {
        captured.push(s);
        return true;
      },
    } as unknown as NodeJS.WriteStream;
    const log = makeLogger(stream, stream);
    log.info('hello');
    assert.ok(captured[0]?.includes('[INFO] hello'));
  });
});
