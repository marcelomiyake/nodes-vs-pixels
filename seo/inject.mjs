#!/usr/bin/env node
// Injects the shared SEO head block into a built index.html and writes robots.txt.
// Both frontends call this same step, so their metadata cannot drift apart.
import { readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { join, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

// The placeholder title of each index.html, which the head block replaces.
export const MARKER = /<title>[^<]*<\/title>/g;

const ESCAPES = { '&': '&amp;', '"': '&quot;', '<': '&lt;', '>': '&gt;' };

export function escapeHtml(value) {
  return String(value).replaceAll(/[&"<>]/g, (char) => ESCAPES[char]);
}

export function resolveUrl(site, app, override) {
  const url = override || site.urls[app];
  if (!url) {
    throw new Error(`Unknown app "${app}"; expected one of: ${Object.keys(site.urls).join(', ')}`);
  }
  return new URL(url).href;
}

export function renderHead(site, url) {
  const meta = (attr, key, content) => `<meta ${attr}="${key}" content="${escapeHtml(content)}">`;
  return [
    `<title>${escapeHtml(site.title)}</title>`,
    meta('name', 'description', site.description),
    meta('name', 'robots', site.robots),
    meta('name', 'theme-color', site.themeColor),
    `<link rel="canonical" href="${escapeHtml(url)}">`,
    meta('property', 'og:type', 'website'),
    meta('property', 'og:site_name', site.siteName),
    meta('property', 'og:locale', site.locale),
    meta('property', 'og:title', site.title),
    meta('property', 'og:description', site.description),
    meta('property', 'og:url', url),
    meta('name', 'twitter:card', 'summary'),
    meta('name', 'twitter:title', site.title),
    meta('name', 'twitter:description', site.description),
  ].join('\n    ');
}

export function injectSeo(html, head) {
  // The canonical link only comes from the head block: a page that has one
  // was processed already, and a second block would duplicate every tag.
  if (html.includes('rel="canonical"')) {
    throw new Error('index.html already has SEO metadata');
  }
  if (html.match(MARKER)?.length !== 1) {
    throw new Error('index.html must contain exactly one <title> to replace');
  }
  return html.replace(MARKER, () => head);
}

// Resolves the build output directory, which must lie inside the working
// directory, so the tool cannot be pointed at files elsewhere.
export function outputDir(dir) {
  const resolved = realpathSync(dir);
  const base = realpathSync(process.cwd());
  if (resolved !== base && !resolved.startsWith(base + sep)) {
    throw new Error(`${dir} is outside the working directory`);
  }
  return resolved;
}

export const ROBOTS_TXT = 'User-agent: *\nAllow: /\n';

export function run(argv, env = process.env) {
  const { values } = parseArgs({
    args: argv,
    options: { app: { type: 'string' }, dir: { type: 'string' } },
  });
  if (!values.app || !values.dir) {
    throw new Error('Usage: inject-seo --app <web-react|web-wasm> --dir <build output dir>');
  }
  const site = JSON.parse(readFileSync(new URL('site.json', import.meta.url), 'utf8'));
  const url = resolveUrl(site, values.app, env.SITE_URL);
  const dir = outputDir(values.dir);
  const indexPath = join(dir, 'index.html');
  writeFileSync(indexPath, injectSeo(readFileSync(indexPath, 'utf8'), renderHead(site, url)));
  writeFileSync(join(dir, 'robots.txt'), ROBOTS_TXT);
  return url;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  console.log(`SEO metadata injected for ${run(process.argv.slice(2))}`);
}
