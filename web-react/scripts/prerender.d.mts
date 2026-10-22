export function insertApp(html: string, markup: string): string;
export function inlineStylesheets(html: string, readStylesheet: (href: string) => string): string;
export function prerender(dist: URL, render: () => string): void;
