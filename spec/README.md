# Form specification

These files are shared by every project, so both frontends and the backend behave the same way.

- `copy.json` holds every user-visible string: labels, placeholders, the password hint, buttons and violation messages.
- `validation-cases.json` holds conformance vectors. Each case merges `input` over `base` and lists the expected violations in form order. `crates/form-core` and `web-react` run every case in their test suites.
- `line-breaks.json` holds line break opportunities recorded from Chrome: each text laid out at `min-content` width, one entry per line. `web-wasm` breaks its canvas text at the same places.

## Validation rules

Lengths count Unicode scalar values (code points). "Whitespace" means the Unicode `White_Space` property. Each field reports at most one violation, which is the first failing rule in the order listed.

| Field | Rules |
| --- | --- |
| `fullName` | Trim whitespace. Empty → `REQUIRED`. Fewer than 2 → `TOO_SHORT`. More than 80 → `TOO_LONG`. Any character other than an `Alphabetic` letter, space, `'`, `’` or `-` → `INVALID_CHARACTERS`. |
| `email` | Trim whitespace. Empty → `REQUIRED`. More than 254 → `TOO_LONG`. It must otherwise have exactly one `@`, a non-empty local part, a domain of at least two non-empty dot-separated labels, and no whitespace, or the result is `INVALID_FORMAT`. |
| `password` | Not trimmed. Empty → `REQUIRED`. Fewer than 12 → `TOO_SHORT`. More than 128 → `TOO_LONG`. It must contain an `Uppercase` letter, a `Lowercase` letter, an ASCII digit and a symbol, or the result is `WEAK`. A symbol is any character that is not `Alphabetic`, not a number (`\p{N}`) and not whitespace. |
| `confirmPassword` | Empty → `REQUIRED`. Not equal to `password` → `MISMATCH`. |
| `acceptTerms` | Not checked → `REQUIRED`. |
