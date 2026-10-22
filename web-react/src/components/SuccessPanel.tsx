import { useEffect, useRef } from 'react';

import copy, { format } from '../copy';

export default function SuccessPanel({ name }: Readonly<{ name: string }>) {
  const heading = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    heading.current?.focus();
  }, []);

  return (
    <section aria-labelledby="success-title" className="flex flex-col items-center text-center">
      <div className="flex size-icon items-center justify-center rounded-full bg-success-soft">
        <svg aria-hidden="true" viewBox="0 0 24 24" className="size-6 text-success">
          <path
            d="M5 12.5l4.5 4.5L19 7.5"
            fill="none"
            stroke="currentColor"
            strokeWidth="2.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </div>
      <h1 id="success-title" ref={heading} tabIndex={-1} className="mt-header-gap text-title text-ink outline-none">
        {copy.success.title}
      </h1>
      <p className="mt-title-gap text-small text-muted">{format(copy.success.body, { name })}</p>
    </section>
  );
}
