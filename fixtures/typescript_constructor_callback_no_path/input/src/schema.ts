import { constructor } from './core';

export const ZodTransform = constructor('ZodTransform', (inst, def) => {
  inst.parse = (payload: { issues: string[]; value: unknown }) => {
    const start = payload.issues.length + 1;
    const output = def.transform(payload.value, payload);
    return { start, output };
  };
});
