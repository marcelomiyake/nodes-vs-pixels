// The sign-up rules from spec/README.md. crates/form-core implements the same
// rules in Rust, and both run the cases in spec/validation-cases.json.

export const FIELDS = ['fullName', 'email', 'password', 'confirmPassword', 'acceptTerms'] as const;

export type FieldKey = (typeof FIELDS)[number];

export type ViolationKey =
  | 'REQUIRED'
  | 'TOO_SHORT'
  | 'TOO_LONG'
  | 'INVALID_CHARACTERS'
  | 'INVALID_FORMAT'
  | 'WEAK'
  | 'MISMATCH';

export interface SignUpValues {
  fullName: string;
  email: string;
  password: string;
  confirmPassword: string;
  acceptTerms: boolean;
}

export interface Violation {
  field: FieldKey;
  code: ViolationKey;
}

const NAME_MIN = 2;
const NAME_MAX = 80;
const EMAIL_MAX = 254;
const PASSWORD_MIN = 12;
const PASSWORD_MAX = 128;

const EDGE_WHITESPACE = /^\p{White_Space}+|\p{White_Space}+$/gu;
const WHITESPACE = /\p{White_Space}/u;
const NAME_CHARACTERS = /^[\p{Alphabetic} '’-]+$/u;
const UPPERCASE = /\p{Uppercase}/u;
const LOWERCASE = /\p{Lowercase}/u;
const DIGIT = /[0-9]/;
const SYMBOL = /[^\p{Alphabetic}\p{N}\p{White_Space}]/u;

const trim = (value: string) => value.replaceAll(EDGE_WHITESPACE, '');

/** Length in Unicode code points, matching Rust's `chars().count()`. */
const codePoints = (value: string) => Array.from(value).length;

function checkLength(value: string, min: number, max: number): ViolationKey | null {
  const length = codePoints(value);
  if (length === 0) return 'REQUIRED';
  if (length < min) return 'TOO_SHORT';
  if (length > max) return 'TOO_LONG';
  return null;
}

function checkFullName(value: string): ViolationKey | null {
  const name = trim(value);
  return checkLength(name, NAME_MIN, NAME_MAX) ?? (NAME_CHARACTERS.test(name) ? null : 'INVALID_CHARACTERS');
}

function isEmailShaped(email: string): boolean {
  const at = email.indexOf('@');
  const domain = email.slice(at + 1);
  const labels = domain.split('.');
  return (
    at > 0 &&
    !WHITESPACE.test(email) &&
    !domain.includes('@') &&
    labels.length >= 2 &&
    labels.every((label) => label.length > 0)
  );
}

function checkEmail(value: string): ViolationKey | null {
  const email = trim(value);
  return checkLength(email, 1, EMAIL_MAX) ?? (isEmailShaped(email) ? null : 'INVALID_FORMAT');
}

function checkPassword(value: string): ViolationKey | null {
  const strong = [UPPERCASE, LOWERCASE, DIGIT, SYMBOL].every((pattern) => pattern.test(value));
  return checkLength(value, PASSWORD_MIN, PASSWORD_MAX) ?? (strong ? null : 'WEAK');
}

function checkConfirmation(password: string, confirmation: string): ViolationKey | null {
  if (confirmation === '') return 'REQUIRED';
  return confirmation === password ? null : 'MISMATCH';
}

/** Returns the first rule `field` fails, or null when it is valid. */
export function checkField(values: SignUpValues, field: FieldKey): ViolationKey | null {
  switch (field) {
    case 'fullName':
      return checkFullName(values.fullName);
    case 'email':
      return checkEmail(values.email);
    case 'password':
      return checkPassword(values.password);
    case 'confirmPassword':
      return checkConfirmation(values.password, values.confirmPassword);
    case 'acceptTerms':
      return values.acceptTerms ? null : 'REQUIRED';
  }
}

/** Validates every field and returns the violations in form order. */
export function validate(values: SignUpValues): Violation[] {
  return FIELDS.flatMap((field) => {
    const code = checkField(values, field);
    return code ? [{ field, code }] : [];
  });
}
