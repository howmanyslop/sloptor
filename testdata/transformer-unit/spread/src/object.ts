const base = { x: 1, y: 2 };
const extra = { z: 3 };
function getObj() {
	return { a: "k", b: 1 };
}
declare const maybe: { x: number } | undefined;
declare const props: { event?: { cb: number } };
const spreadOnly = { ...base };
const spreadFirst = { ...base, z: 3 };
const spreadLast = { z: 3, ...base };
const spreadTwice = { ...base, ...extra };
const spreadMaybe = { ...maybe, q: 1 };
const spreadOptionalProp = { a: 1, ...props.event };
const spreadCall = { ...getObj(), b: 2 };
const computedAfterSpread = { ...base, [getObj().a]: 9 };
print(spreadOnly, spreadFirst, spreadLast, spreadTwice, spreadMaybe, spreadOptionalProp, spreadCall, computedAfterSpread);
