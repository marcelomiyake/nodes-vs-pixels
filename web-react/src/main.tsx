import { StrictMode } from 'react';
import { createRoot, hydrateRoot } from 'react-dom/client';

import App from './App';
import './index.css';

const root = document.getElementById('root');
if (!root) throw new Error('Missing #root element');

const app = (
  <StrictMode>
    <App />
  </StrictMode>
);

// Production builds ship prerendered markup to hydrate; the dev server does not.
if (root.firstElementChild) {
  hydrateRoot(root, app);
} else {
  createRoot(root).render(app);
}
