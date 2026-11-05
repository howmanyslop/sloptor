const tbl = new Map<string, { x: number }>();
const gx = tbl.get("k")?.x;
declare const s: Set<number> | undefined;
s?.add(1);
print(gx, s);
