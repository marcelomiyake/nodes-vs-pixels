import { readFileSync } from 'node:fs';

import { expect, test } from '@playwright/test';

interface Case {
  text: string;
  lines: string[];
}

const { cases } = JSON.parse(readFileSync(new URL('../../spec/line-breaks.json', import.meta.url), 'utf8')) as {
  cases: Case[];
};

// web-wasm wraps canvas text with these recorded breaks, so they must stay
// what Chrome does: each text laid out at min-content width, one entry per line.
test('Chrome still breaks lines as spec/line-breaks.json records', async ({ page }) => {
  await page.setContent('<!doctype html><html lang="en"><body></body></html>');
  const lines = await page.evaluate((texts) => {
    const div = document.createElement('div');
    div.style.cssText = 'width:0;font:14px sans-serif;white-space:normal';
    document.body.append(div);
    return texts.map((text) => {
      div.textContent = text;
      const node = div.firstChild as Text;
      const found: string[] = [];
      let top: number | null = null;
      for (let i = 0; i < text.length; ) {
        const length = (text.codePointAt(i) ?? 0) > 0xffff ? 2 : 1;
        const char = text.slice(i, i + length);
        const range = document.createRange();
        range.setStart(node, i);
        range.setEnd(node, i + length);
        const rect = Array.from(range.getClientRects()).find((r) => r.width > 0) ?? range.getBoundingClientRect();
        if (char !== ' ' && top !== null && Math.abs(rect.top - top) > 1) found.push('');
        if (found.length === 0) found.push('');
        found[found.length - 1] += char;
        if (char !== ' ') top = rect.top;
        i += length;
      }
      return found.map((line) => line.replace(/^ +| +$/g, ''));
    });
  }, cases.map((c) => c.text));
  expect(lines).toEqual(cases.map((c) => c.lines));
});
