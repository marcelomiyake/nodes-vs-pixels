const BOX =
  'peer size-check appearance-none rounded-checkbox border bg-surface outline-none ' +
  'checked:border-brand checked:bg-brand focus-visible:ring-(length:--spacing-ring)';
const NORMAL = 'border-field focus-visible:ring-brand-ring';
const INVALID = 'border-danger focus-visible:ring-danger-ring';

interface TermsCheckboxProps {
  label: string;
  checked: boolean;
  error: string | null;
  readOnly: boolean;
  onChange: (checked: boolean) => void;
  onBlur: () => void;
}

export const CHECK_PATH = 'M5.5 10.5l3 3 6-7';

export default function TermsCheckbox({
  label,
  checked,
  error,
  readOnly,
  onChange,
  onBlur,
}: Readonly<TermsCheckboxProps>) {
  return (
    <div>
      <div className="flex items-start gap-check-gap">
        <span className="relative flex shrink-0">
          <input
            id="acceptTerms"
            name="acceptTerms"
            type="checkbox"
            checked={checked}
            aria-disabled={readOnly}
            aria-invalid={error !== null}
            aria-describedby={error ? 'acceptTerms-message' : undefined}
            className={`${BOX} ${error ? INVALID : NORMAL}`}
            onChange={(event) => {
              onChange(event.target.checked);
            }}
            onBlur={onBlur}
          />
          <svg
            aria-hidden="true"
            viewBox="0 0 20 20"
            className="pointer-events-none absolute inset-0 hidden size-check text-on-brand peer-checked:block"
          >
            <path
              d={CHECK_PATH}
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </span>
        <label htmlFor="acceptTerms" className="text-small text-ink-soft">
          {label}
        </label>
      </div>
      {error && (
        <p id="acceptTerms-message" className="mt-label-gap text-small text-danger">
          {error}
        </p>
      )}
    </div>
  );
}
