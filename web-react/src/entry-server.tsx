import { StrictMode } from 'react';
import { renderToString } from 'react-dom/server';

import App from './App';

/** Renders the initial page at build time so it paints before any JavaScript runs. */
export function render(): string {
  return renderToString(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
