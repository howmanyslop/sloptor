function _Comp() {
	return <frame />;
}
const a = <_Comp />;

function Item(props: { text?: string }) {
	return <frame />;
}
const NS = { Item: Item };
const Nested = { Deep: { Comp: Item } };
const h = <NS.Item text="3" />;
const i = <Nested.Deep.Comp />;
const n = <a:b c:d="x" />;
const k = <frame>{}</frame>;
declare const arr: Array<ReactElement>;
const j = <frame>{...arr}</frame>;
const f = <></>;
declare const children: ReactElement;
const kk = <>{children}</>;
print(a, h, i, n, k, j, f, kk);
