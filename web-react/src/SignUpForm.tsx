import type { SubmitEvent } from 'react';

import BrandMark from './components/BrandMark';
import TermsCheckbox from './components/TermsCheckbox';
import TextField from './components/TextField';
import copy, { violationMessage } from './copy';
import type { FieldKey } from './validation';
import type { useSignUpForm } from './useSignUpForm';

const TEXT_FIELDS = ['fullName', 'email', 'password', 'confirmPassword'] as const;

interface SignUpFormProps {
  form: ReturnType<typeof useSignUpForm>;
}

export default function SignUpForm({ form }: Readonly<SignUpFormProps>) {
  const message = (field: FieldKey) => {
    const code = form.errorFor(field);
    return code ? violationMessage(field, code) : null;
  };

  const onSubmit = (event: SubmitEvent<HTMLFormElement>) => {
    event.preventDefault();
    void form.submit().then((field) => {
      if (field) document.getElementById(field)?.focus();
    });
  };

  const submitting = form.status === 'submitting';

  return (
    <>
      <BrandMark />
      <h1 id="title" className="mt-header-gap text-title text-ink">
        {copy.title}
      </h1>
      <p className="mt-title-gap text-small text-muted">{copy.subtitle}</p>
      <form
        noValidate
        aria-labelledby="title"
        aria-busy={submitting}
        className="mt-form-gap flex flex-col gap-field-gap"
        onSubmit={onSubmit}
      >
        {TEXT_FIELDS.map((field) => {
          const spec = copy.fields[field];
          return (
            <TextField
              key={field}
              id={field}
              label={spec.label}
              type={spec.type}
              autoComplete={spec.autocomplete}
              placeholder={spec.placeholder}
              hint={'hint' in spec ? spec.hint : undefined}
              value={form.values[field]}
              readOnly={form.locked}
              error={message(field)}
              onChange={(value) => {
                form.setText(field, value);
              }}
              onBlur={() => {
                form.blur(field);
              }}
            />
          );
        })}
        <TermsCheckbox
          label={copy.terms}
          checked={form.values.acceptTerms}
          readOnly={form.locked}
          error={message('acceptTerms')}
          onChange={form.setAcceptTerms}
          onBlur={() => {
            form.blur('acceptTerms');
          }}
        />
        <button
          type="submit"
          disabled={submitting}
          className="h-control w-full rounded-control bg-brand text-button text-on-brand outline-none hover:bg-brand-strong focus-visible:ring-(length:--spacing-ring) focus-visible:ring-brand-ring disabled:cursor-wait"
        >
          {submitting ? copy.submitting : copy.submit}
        </button>
        {form.status === 'failed' && (
          <p role="alert" className="text-small text-danger">
            {copy.networkError}
          </p>
        )}
      </form>
    </>
  );
}
