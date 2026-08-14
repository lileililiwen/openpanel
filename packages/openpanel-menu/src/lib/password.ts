/**
 * Password validator.
 *
 * The spec mandates:
 *  - ≥ 12 characters.
 *  - At least one letter and one digit.
 *  - Plaintext never appears in any log line; this module
 *    therefore never logs the candidate.
 */

export class PasswordError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'PasswordError';
  }
}

/** True iff `value` satisfies the panel's password policy. */
export function isStrongPassword(value: string): boolean {
  if (value.length < 12) {
    return false;
  }
  let hasLetter = false;
  let hasDigit = false;
  for (const ch of value) {
    if (/[A-Za-z]/.test(ch)) {
      hasLetter = true;
    }
    if (/[0-9]/.test(ch)) {
      hasDigit = true;
    }
    if (hasLetter && hasDigit) {
      return true;
    }
  }
  return false;
}

/** Throw `PasswordError` when the candidate is invalid. */
export function ensureStrongPassword(value: string): void {
  if (!isStrongPassword(value)) {
    throw new PasswordError('密码必须 ≥ 12 字符,且至少包含一个字母和一个数字.');
  }
}
