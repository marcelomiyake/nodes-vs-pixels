import { describe, expect, it } from 'vitest';

import { render } from './entry-server';

describe('render', () => {
  it('server-renders the sign-up form', () => {
    const html = render();
    expect(html).toContain('<h1 id="title"');
    expect(html).toContain('Create your account');
    expect(html).toContain('name="acceptTerms"');
  });

  it('never submits credentials in the URL before hydration', () => {
    const page = new DOMParser().parseFromString(render(), 'text/html');
    expect(page.querySelector('form')?.method).toBe('post');
  });
});
