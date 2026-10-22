// Bakes the server-rendered app and its stylesheet into dist/client/index.html,
// so the first paint needs only the HTML document and the font.
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';

const APP_MARKER = '<!--app-->';
const STYLESHEET = /<link rel="stylesheet"[^>]*href="([^"]+)"[^>]*>/g;

export function insertApp(html, markup) {
  if (!html.includes(APP_MARKER)) throw new Error(`index.html is missing ${APP_MARKER}`);
  return html.replace(APP_MARKER, () => markup);
}

export function inlineStylesheets(html, readStylesheet) {
  return html.replaceAll(STYLESHEET, (_, href) => `<style>${readStylesheet(href)}</style>`);
}

export function prerender(dist, render) {
  const indexUrl = new URL('client/index.html', dist);
  const html = inlineStylesheets(insertApp(readFileSync(indexUrl, 'utf8'), render()), (href) =>
    readFileSync(new URL(`client${href}`, dist), 'utf8'),
  );
  writeFileSync(indexUrl, html);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const dist = pathToFileURL(`${process.cwd()}/dist/`);
  const { render } = await import(new URL('server/entry-server.js', dist).href);
  prerender(dist, render);
}
