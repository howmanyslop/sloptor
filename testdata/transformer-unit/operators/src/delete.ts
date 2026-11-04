declare const obj: { foo?: number; [key: string]: number | undefined };
declare const key: string;

delete obj[key];
const deleted = delete obj.foo;
