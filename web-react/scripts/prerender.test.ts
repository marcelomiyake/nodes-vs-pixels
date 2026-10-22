import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { describe, expect, it } from 'vitest';

import { inlineStylesheets, insertApp, prerender } from './prerender.mjs';

describe('insertApp', () => {
  it('replaces the app marker with the markup', () => {
    expect(insertApp('<div id="root"><!--app--></div>', '<main>$&</main>')).toBe(
      '<div id="root"><main>$&</main></div>',
    );
  });

  it('requires the marker', () => {
    expect(() => insertApp('<div id="root"></div>', '')).toThrow('missing <!--app-->');
  });
});

describe('inlineStylesheets', () => {
  it('swaps stylesheet links for inline styles', () => {
    const html = '<head><link rel="stylesheet" crossorigin href="/assets/a.css"></head>';
    expect(inlineStylesheets(html, (href) => `/* ${href} */`)).toBe('<head><style>/* /assets/a.css */</style></head>');
  });
});

describe('prerender', () => {
  it('writes the rendered app and inlined CSS into the client index', () => {
    const dir = mkdtempSync(join(tmpdir(), 'prerender-'));
    try {
      mkdirSync(join(dir, 'client/assets'), { recursive: true });
      writeFileSync(
        join(dir, 'client/index.html'),
        '<link rel="stylesheet" href="/assets/app.css"><div id="root"><!--app--></div>',
      );
      writeFileSync(join(dir, 'client/assets/app.css'), 'body{}');

      prerender(pathToFileURL(`${dir}/`), () => '<main>hi</main>');

      expect(readFileSync(join(dir, 'client/index.html'), 'utf8')).toBe(
        '<style>body{}</style><div id="root"><main>hi</main></div>',
      );
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
