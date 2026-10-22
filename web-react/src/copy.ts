import copy from '../../spec/copy.json';
import type { FieldKey, ViolationKey } from './validation';

type Messages = Record<FieldKey, Partial<Record<ViolationKey, string>>>;

const messages: Messages = copy.violations;

/** The user-facing message for a violation, from spec/copy.json. */
export function violationMessage(field: FieldKey, code: ViolationKey): string {
  return messages[field][code] ?? code;
}

/** Fills `{name}`-style placeholders in a copy string. */
export function format(template: string, values: Record<string, string>): string {
  return template.replaceAll(/\{(\w+)\}/g, (match, key: string) => values[key] ?? match);
}

export default copy;
