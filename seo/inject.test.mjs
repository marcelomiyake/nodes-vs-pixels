import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, it } from 'node:test';
import { fileURLToPath } from 'node:url';

import {
  MARKER,
  ROBOTS_TXT,
  escapeHtml,
  injectSeo,
  renderHead,
  resolveUrl,
  run,
} from './inject.mjs';

const site = {
  siteName: 'Site',
  title: 'Title & "more"',
  description: 'Description <b>',
  locale: 'en_US',
  themeColor: '#000000',
  robots: 'index, follow',
  urls: { 'web-react': 'http://localhost:8081/', 'web-wasm': 'http://localhost:8082/' },
};

describe('escapeHtml', () => {
  it('escapes characters that break attributes and markup', () => {
    assert.equal(escapeHtml('a & "b" <c>'), 'a &amp; &quot;b&quot; &lt;c&gt;');
  });
});

describe('resolveUrl', () => {
  it('uses the configured URL of the app', () => {
    assert.equal(resolveUrl(site, 'web-wasm'), 'http://localhost:8082/');
  });

  it('prefers an override and normalizes it', () => {
    assert.equal(resolveUrl(site, 'web-react', 'https://example.com'), 'https://example.com/');
  });

  it('rejects unknown apps', () => {
    assert.throws(() => resolveUrl(site, 'nope'), /Unknown app "nope"/);
  });
});

describe('renderHead', () => {
  const head = renderHead(site, 'http://localhost:8081/');

  it('renders an escaped title and description', () => {
    assert.match(head, /<title>Title &amp; &quot;more&quot;<\/title>/);
    assert.match(head, /<meta name="description" content="Description &lt;b&gt;">/);
  });

  it('renders canonical, Open Graph and Twitter tags', () => {
    assert.match(head, /<link rel="canonical" href="http:\/\/localhost:8081\/">/);
    assert.match(head, /<meta property="og:url" content="http:\/\/localhost:8081\/">/);
    assert.match(head, /<meta name="twitter:card" content="summary">/);
    assert.match(head, /<meta name="theme-color" content="#000000">/);
  });
});

describe('injectSeo', () => {
  it('replaces the marker with the head block', () => {
    assert.equal(injectSeo(`<head>${MARKER}</head>`, '<title>$&</title>'), '<head><title>$&</title></head>');
  });

  it('requires exactly one marker', () => {
    assert.throws(() => injectSeo('<head></head>', ''), /exactly once/);
    assert.throws(() => injectSeo(`${MARKER}${MARKER}`, ''), /exactly once/);
  });
});

describe('run', () => {
  let dir;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'seo-'));
    writeFileSync(join(dir, 'index.html'), `<head>\n    ${MARKER}\n</head>`);
  });

  afterEach(() => rmSync(dir, { recursive: true, force: true }));

  it('injects metadata and writes robots.txt', () => {
    assert.equal(run(['--app', 'web-react', '--dir', dir], {}), 'http://localhost:8081/');
    const html = readFileSync(join(dir, 'index.html'), 'utf8');
    assert.match(html, /<title>Create your account · Nodes vs Pixels<\/title>/);
    assert.doesNotMatch(html, new RegExp(MARKER));
    assert.equal(readFileSync(join(dir, 'robots.txt'), 'utf8'), ROBOTS_TXT);
  });

  it('honours SITE_URL', () => {
    assert.equal(run(['--app', 'web-wasm', '--dir', dir], { SITE_URL: 'https://pixels.example' }), 'https://pixels.example/');
  });

  it('requires both arguments', () => {
    assert.throws(() => run(['--app', 'web-react'], {}), /Usage/);
  });

  it('works as a command-line tool', () => {
    const script = fileURLToPath(new URL('inject.mjs', import.meta.url));
    const result = spawnSync(process.execPath, [script, '--app', 'web-wasm', '--dir', dir], {
      encoding: 'utf8',
      env: { ...process.env, SITE_URL: '' },
    });
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /SEO metadata injected for http:\/\/localhost:8082\//);
  });
});
