import { describe, expect, it } from 'vitest';

import spec from '../../spec/validation-cases.json';
import { FIELDS, checkField, validate, type SignUpValues } from './validation';

describe('validation conformance', () => {
  it.each(spec.cases)('$name', ({ input, violations }) => {
    const values: SignUpValues = { ...spec.base, ...input };
    expect(validate(values)).toEqual(violations);
  });
});

describe('checkField', () => {
  it('checks a single field', () => {
    const values: SignUpValues = { ...spec.base, email: 'ada@example' };
    expect(checkField(values, 'email')).toBe('INVALID_FORMAT');
    expect(checkField(values, 'fullName')).toBeNull();
  });

  it('lists fields in form order', () => {
    expect(FIELDS).toEqual(['fullName', 'email', 'password', 'confirmPassword', 'acceptTerms']);
  });
});
