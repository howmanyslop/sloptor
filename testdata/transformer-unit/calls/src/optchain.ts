interface Obj {
	b: number;
	fn: () => number;
}
declare const a: Obj | undefined;
declare const o: { cb: (() => number) | undefined };
declare const om: { m?(n: number): number };
declare const arr: Array<number> | undefined;

const r1 = a?.b;
const r2 = a?.fn();
o.cb?.();
const r3 = o.cb?.();
om.m?.(1);
const r4 = om.m?.(2);
arr?.pop();
const r5 = arr?.pop();
print(r1, r2, r3, r4, r5);
