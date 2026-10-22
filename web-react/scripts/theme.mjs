// Generates the Tailwind theme from design/tokens.json and copies the shared
// font and favicon into public/, so React uses exactly the canvas app's values.
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const px = (value) => `${value}px`;

export function renderTheme(tokens) {
  const lines = [];
  const add = (name, value) => lines.push(`  --${name}: ${value};`);

  add('font-sans', `'${tokens.font.family}', ${tokens.font.fallback}`);
  for (const [name, value] of Object.entries(tokens.color)) add(`color-${name}`, value);
  for (const [name, { size, lineHeight, weight }] of Object.entries(tokens.text)) {
    add(`text-${name}`, px(size));
    add(`text-${name}--line-height`, px(lineHeight));
    add(`text-${name}--font-weight`, weight);
  }
  for (const [name, value] of Object.entries(tokens.radius)) add(`radius-${name}`, px(value));
  for (const [name, { x, y, blur, color }] of Object.entries(tokens.shadow)) {
    add(`shadow-${name}`, `${px(x)} ${px(y)} ${px(blur)} ${color}`);
  }
  for (const [name, value] of Object.entries(tokens.spacing)) add(`spacing-${name}`, px(value));
  for (const [name, value] of Object.entries(tokens.container)) add(`container-${name}`, px(value));
  for (const [name, value] of Object.entries(tokens.breakpoint)) add(`breakpoint-${name}`, px(value));

  return [
    '/* Generated from design/tokens.json by scripts/theme.mjs. Do not edit. */',
    '@font-face {',
    `  font-family: '${tokens.font.family}';`,
    `  src: url('/fonts/${tokens.font.file}') format('woff2');`,
    `  font-weight: ${tokens.font.weightRange};`,
    '  font-style: normal;',
    '  font-display: block;',
    '}',
    '',
    '@theme {',
    ...lines,
    '}',
    '',
  ].join('\n');
}

export function generate(root = new URL('../', import.meta.url)) {
  const design = new URL('../design/', root);
  const tokens = JSON.parse(readFileSync(new URL('tokens.json', design), 'utf8'));
  mkdirSync(new URL('src/generated/', root), { recursive: true });
  writeFileSync(new URL('src/generated/theme.css', root), renderTheme(tokens));
  mkdirSync(new URL('public/fonts/', root), { recursive: true });
  copyFileSync(new URL(`fonts/${tokens.font.file}`, design), new URL(`public/fonts/${tokens.font.file}`, root));
  copyFileSync(new URL('favicon.svg', design), new URL('public/favicon.svg', root));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  generate();
}
