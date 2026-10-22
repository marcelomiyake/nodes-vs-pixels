import type { ChangeEvent } from 'react';

const BASE =
  'mt-label-gap block h-control w-full rounded-control border bg-surface px-control-x text-body text-ink ' +
  'outline-none placeholder:text-subtle focus:ring-(length:--spacing-ring)';
const NORMAL = 'border-field focus:border-brand focus:ring-brand-ring';
const INVALID = 'border-danger focus:ring-danger-ring';

interface TextFieldProps {
  id: string;
  label: string;
  type: string;
  autoComplete: string;
  placeholder: string;
  value: string;
  error: string | null;
  hint?: string;
  readOnly: boolean;
  onChange: (value: string) => void;
  onBlur: () => void;
}

export default function TextField({
  id,
  label,
  type,
  autoComplete,
  placeholder,
  value,
  error,
  hint,
  readOnly,
  onChange,
  onBlur,
}: Readonly<TextFieldProps>) {
  const message = error ?? hint;
  const messageId = `${id}-message`;
  return (
    <div>
      <label htmlFor={id} className="block text-label text-ink-soft">
        {label}
      </label>
      <input
        id={id}
        name={id}
        type={type}
        autoComplete={autoComplete}
        placeholder={placeholder}
        value={value}
        readOnly={readOnly}
        aria-invalid={error !== null}
        aria-describedby={message ? messageId : undefined}
        className={`${BASE} ${error ? INVALID : NORMAL}`}
        onChange={(event: ChangeEvent<HTMLInputElement>) => {
          onChange(event.target.value);
        }}
        onBlur={onBlur}
      />
      {message && (
        <p id={messageId} className={`mt-label-gap text-small ${error ? 'text-danger' : 'text-subtle'}`}>
          {message}
        </p>
      )}
    </div>
  );
}
