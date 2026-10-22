import { createClient, type Transport } from '@connectrpc/connect';
import { createGrpcWebTransport } from '@connectrpc/connect-web';

import { Field, SignupService, ViolationCode, type SignUpResponse } from './gen/signup/v1/signup_pb';
import type { FieldKey, SignUpValues, Violation, ViolationKey } from './validation';

export type SignUpResult = { ok: true } | { ok: false; violations: Violation[] };

export type SignUp = (values: SignUpValues) => Promise<SignUpResult>;

const FIELD_KEYS: Partial<Record<Field, FieldKey>> = {
  [Field.FULL_NAME]: 'fullName',
  [Field.EMAIL]: 'email',
  [Field.PASSWORD]: 'password',
  [Field.CONFIRM_PASSWORD]: 'confirmPassword',
  [Field.ACCEPT_TERMS]: 'acceptTerms',
};

const VIOLATION_KEYS: Partial<Record<ViolationCode, ViolationKey>> = {
  [ViolationCode.REQUIRED]: 'REQUIRED',
  [ViolationCode.TOO_SHORT]: 'TOO_SHORT',
  [ViolationCode.TOO_LONG]: 'TOO_LONG',
  [ViolationCode.INVALID_CHARACTERS]: 'INVALID_CHARACTERS',
  [ViolationCode.INVALID_FORMAT]: 'INVALID_FORMAT',
  [ViolationCode.WEAK]: 'WEAK',
  [ViolationCode.MISMATCH]: 'MISMATCH',
};

/** Converts a response to a result, dropping violations this client does not understand. */
export function toResult(response: SignUpResponse): SignUpResult {
  if (response.violations.length === 0) return { ok: true };
  const violations = response.violations.flatMap(({ field, code }) => {
    const fieldKey = FIELD_KEYS[field];
    const codeKey = VIOLATION_KEYS[code];
    return fieldKey && codeKey ? [{ field: fieldKey, code: codeKey }] : [];
  });
  return { ok: false, violations };
}

/** Creates a sign-up call over the given Connect transport. */
export function createSignUp(transport: Transport): SignUp {
  const client = createClient(SignupService, transport);
  return async (values) => toResult(await client.signUp(values));
}

let backend: SignUp | undefined;

/** Calls the backend over gRPC-web on the page's own origin (proxied to the backend). */
export const signUpViaBackend: SignUp = (values) => {
  backend ??= createSignUp(createGrpcWebTransport({ baseUrl: globalThis.location.origin }));
  return backend(values);
};
