declare const source: object;
declare const maybeObject: object | undefined;
const first = <frame {...source} />;
const last = <frame Visible={true} {...source} />;
const optional = <frame {...maybeObject} />;
print(first, last, optional);
