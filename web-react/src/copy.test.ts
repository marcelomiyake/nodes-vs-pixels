import { describe, expect, it } from 'vitest';

import copy, { format, violationMessage } from './copy';

describe('violationMessage', () => {
  it('returns the shared message for a violation', () => {
    expect(violationMessage('confirmPassword', 'MISMATCH')).toBe("Passwords don't match.");
  });

  it('falls back to the code when no message exists', () => {
    expect(violationMessage('acceptTerms', 'WEAK')).toBe('WEAK');
  });
});

describe('format', () => {
  it('fills known placeholders and keeps unknown ones', () => {
    expect(format(copy.success.body, { name: 'Ada' })).toBe('Welcome aboard, Ada. Your account is ready.');
    expect(format('{missing}', {})).toBe('{missing}');
  });
});
