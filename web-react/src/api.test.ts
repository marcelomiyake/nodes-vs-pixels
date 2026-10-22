import { create } from '@bufbuild/protobuf';
import { createRouterTransport } from '@connectrpc/connect';
import { describe, expect, it, vi } from 'vitest';

import { createSignUp, signUpViaBackend, toResult } from './api';
import { Field, SignUpResponseSchema, SignupService, ViolationCode } from './gen/signup/v1/signup_pb';

const values = {
  fullName: 'Ada Lovelace',
  email: 'ada@example.com',
  password: 'Analytical#Engine1',
  confirmPassword: 'Analytical#Engine1',
  acceptTerms: true,
};

describe('toResult', () => {
  it('treats a response without violations as success', () => {
    expect(toResult(create(SignUpResponseSchema, { accountId: 'id' }))).toEqual({ ok: true });
  });

  it('maps known violations and drops unknown ones', () => {
    const response = create(SignUpResponseSchema, {
      violations: [
        { field: Field.EMAIL, code: ViolationCode.INVALID_FORMAT },
        { field: Field.UNSPECIFIED, code: ViolationCode.REQUIRED },
        { field: Field.PASSWORD, code: ViolationCode.UNSPECIFIED },
      ],
    });
    expect(toResult(response)).toEqual({ ok: false, violations: [{ field: 'email', code: 'INVALID_FORMAT' }] });
  });
});

describe('createSignUp', () => {
  it('sends the form values to SignupService.SignUp', async () => {
    const received: unknown[] = [];
    const transport = createRouterTransport(({ service }) => {
      service(SignupService, {
        signUp(request) {
          received.push({ ...request });
          return { violations: [{ field: Field.CONFIRM_PASSWORD, code: ViolationCode.MISMATCH }] };
        },
      });
    });

    const result = await createSignUp(transport)(values);

    expect(result).toEqual({ ok: false, violations: [{ field: 'confirmPassword', code: 'MISMATCH' }] });
    expect(received).toEqual([expect.objectContaining(values)]);
  });
});

describe('signUpViaBackend', () => {
  it('posts gRPC-web to the page origin', async () => {
    const fetchMock = vi.fn<typeof fetch>().mockRejectedValue(new TypeError('offline'));
    vi.stubGlobal('fetch', fetchMock);
    try {
      await expect(signUpViaBackend(values)).rejects.toThrow('offline');
      const [url, init] = fetchMock.mock.calls[0] ?? [];
      expect(url).toBe(`${location.origin}/signup.v1.SignupService/SignUp`);
      expect(new Headers(init?.headers).get('content-type')).toBe('application/grpc-web+proto');
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
