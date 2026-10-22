import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { describe, expect, it } from 'vitest';

import tokens from '../../design/tokens.json';
import { generate, renderTheme } from './theme.mjs';

describe('renderTheme', () => {
  const css = renderTheme(tokens);

  it('declares the self-hosted font', () => {
    expect(css).toContain("src: url('/fonts/inter-latin.woff2') format('woff2');");
    expect(css).toContain('--font-sans: \'Inter\', ui-sans-serif, system-ui, sans-serif;');
  });

  it('maps every token group to a Tailwind theme variable', () => {
    expect(css).toContain('--color-brand: #4f46e5;');
    expect(css).toContain('--text-title: 24px;');
    expect(css).toContain('--text-title--line-height: 32px;');
    expect(css).toContain('--text-title--font-weight: 600;');
    expect(css).toContain('--radius-card: 16px;');
    expect(css).toContain('--shadow-card: 0px 4px 24px rgba(15, 23, 42, 0.08);');
    expect(css).toContain('--spacing-control: 44px;');
    expect(css).toContain('--container-card: 448px;');
    expect(css).toContain('--breakpoint-sm: 640px;');
  });
});

describe('generate', () => {
  it('writes the theme and copies shared assets', () => {
    const dir = mkdtempSync(join(tmpdir(), 'theme-'));
    try {
      const design = join(dir, 'design');
      mkdirSync(join(design, 'fonts'), { recursive: true });
      writeFileSync(join(design, 'tokens.json'), JSON.stringify(tokens));
      writeFileSync(join(design, 'fonts', 'inter-latin.woff2'), 'font');
      writeFileSync(join(design, 'favicon.svg'), '<svg/>');
      const root = join(dir, 'web-react');
      mkdirSync(root);

      generate(pathToFileURL(`${root}/`));

      expect(readFileSync(join(root, 'src/generated/theme.css'), 'utf8')).toBe(renderTheme(tokens));
      expect(readFileSync(join(root, 'public/fonts/inter-latin.woff2'), 'utf8')).toBe('font');
      expect(readFileSync(join(root, 'public/favicon.svg'), 'utf8')).toBe('<svg/>');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
