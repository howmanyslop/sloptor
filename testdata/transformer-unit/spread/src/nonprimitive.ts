declare const source: object;
declare const maybeObject: object | undefined;
const first = { ...source };
const last = { n: 1, ...source };
const optional = { ...maybeObject };
print(first, last, optional);
