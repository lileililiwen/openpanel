import { test, describe } from 'node:test';
import assert from 'node:assert/strict';
import { parsePort, ConfigError } from '../src/lib/config.js';

describe('parsePort', () => {
  test('parses a simple "port = NNNN" line', () => {
    assert.equal(parsePort('port = 8080\n'), 8080);
  });

  test('parses inside a [server] section', () => {
    assert.equal(parsePort('[server]\nport = 9090\n'), 9090);
  });

  test('case-insensitive', () => {
    assert.equal(parsePort('PORT = 7777\n'), 7777);
  });

  test('rejects non-numeric', () => {
    assert.throws(() => parsePort('port = abc\n'), ConfigError);
  });

  test('range boundaries: 1, 65535, 65536', () => {
    assert.equal(parsePort('port = 1\n'), 1);
    assert.equal(parsePort('port = 65535\n'), 65535);
    assert.throws(() => parsePort('port = 65536\n'), ConfigError);
  });

  test('comments are tolerated', () => {
    assert.equal(parsePort('# leading comment\nport = 8080 # trailing\n'), 8080);
  });

  test('throws on missing port', () => {
    assert.throws(() => parsePort('host = 0.0.0.0\n'), ConfigError);
  });
});
