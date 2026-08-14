import { test, describe } from 'node:test';
import assert from 'node:assert/strict';
import { isStrongPassword, ensureStrongPassword, PasswordError } from '../src/lib/password.js';

describe('isStrongPassword', () => {
  test('12 chars accepted when letter + digit', () => {
    assert.equal(isStrongPassword('Aa1aaaaaaaaa'), true);
  });

  test('11 chars rejected', () => {
    assert.equal(isStrongPassword('Aa1aaaaaaaa'), false);
  });

  test('12 digits rejected', () => {
    assert.equal(isStrongPassword('123456789012'), false);
  });

  test('12 letters rejected', () => {
    assert.equal(isStrongPassword('abcdefghijkl'), false);
  });

  test('typical strong password accepted', () => {
    assert.equal(isStrongPassword('correct horse battery 9'), true);
  });

  test('ensureStrongPassword throws on weak', () => {
    assert.throws(() => ensureStrongPassword('short'), PasswordError);
  });

  test('ensureStrongPassword passes on strong', () => {
    ensureStrongPassword('Aa1aaaaaaaaa');
  });
});
