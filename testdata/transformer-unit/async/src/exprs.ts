const fexpr = async function (x: number) {
	return x;
};
const genExpr = function* () {
	yield 5;
};
function* inner() {
	yield 1;
	return "r";
}
function* outerStmt() {
	yield* inner();
	yield;
}
export async function chainBin(a: () => Promise<number>) {
	const x = 1 + (await a());
	if ((await a()) > 0) {
		await a();
	}
	await a();
	return x;
}
const arrowExprBody = async (n: number) => n + (await Promise.resolve(1));
export function resume() {
	const g = (function* (): Generator<number, void, number> {
		const got = yield 1;
		print(got);
	})();
	g.next();
	g.next(5);
}
export function use() {
	print(fexpr(2), genExpr(), outerStmt(), arrowExprBody(1));
}
