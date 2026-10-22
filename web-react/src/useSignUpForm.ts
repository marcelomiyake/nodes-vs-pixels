import { useState } from 'react';

import type { SignUp } from './api';
import { FIELDS, checkField, validate, type FieldKey, type SignUpValues, type ViolationKey } from './validation';

export type Status = 'editing' | 'submitting' | 'success' | 'failed';

type TextFieldKey = Exclude<FieldKey, 'acceptTerms'>;

const EMPTY: SignUpValues = {
  fullName: '',
  email: '',
  password: '',
  confirmPassword: '',
  acceptTerms: false,
};

/**
 * Form state shared by every field. A field shows its error once it has been
 * blurred or the form was submitted. Server violations override the client
 * check until that field changes again. Values are locked while a submission
 * is pending, so the response always matches what is on screen.
 */
export function useSignUpForm(signUp: SignUp) {
  const [values, setValues] = useState(EMPTY);
  const [touched, setTouched] = useState<ReadonlySet<FieldKey>>(new Set());
  const [serverErrors, setServerErrors] = useState<Partial<Record<FieldKey, ViolationKey>>>({});
  const [status, setStatus] = useState<Status>('editing');

  const errorFor = (field: FieldKey): ViolationKey | null =>
    touched.has(field) ? (serverErrors[field] ?? checkField(values, field)) : null;

  const locked = status === 'submitting';

  const update = (next: Partial<SignUpValues>, field: FieldKey) => {
    if (locked) return;
    setValues((current) => ({ ...current, ...next }));
    setServerErrors((current) => ({ ...current, [field]: undefined }));
  };

  const setText = (field: TextFieldKey, value: string) => {
    update({ [field]: value }, field);
  };

  const setAcceptTerms = (acceptTerms: boolean) => {
    update({ acceptTerms }, 'acceptTerms');
  };

  const blur = (field: FieldKey) => {
    setTouched((current) => new Set(current).add(field));
  };

  /** Submits the form and returns the first field that needs attention, if any. */
  const submit = async (): Promise<FieldKey | null> => {
    setTouched(new Set(FIELDS));
    const [firstViolation] = validate(values);
    if (firstViolation) return firstViolation.field;

    setStatus('submitting');
    try {
      const result = await signUp(values);
      if (result.ok) {
        setStatus('success');
        return null;
      }
      const [first] = result.violations;
      setServerErrors(Object.fromEntries(result.violations.map(({ field, code }) => [field, code])));
      setStatus(first ? 'editing' : 'failed');
      return first?.field ?? null;
    } catch {
      setStatus('failed');
      return null;
    }
  };

  return { values, status, locked, errorFor, setText, setAcceptTerms, blur, submit };
}
