export function f1() {
	try {
		print("try");
	} catch (e) {
		print("caught", e);
	}
}
export function f2() {
	try {
		print("try");
	} finally {
		print("finally");
	}
}
export function f4() {
	try {
		print("x");
	} catch {
		print("no binding");
	}
}
export function throwInTry() {
	try {
		throw "err";
	} catch (e) {
		print(e);
	}
}
export function catchDestructure() {
	try {
		print("t");
	} catch (e) {
		const { name } = e as { name: string };
		print(name);
	}
}
export function tcf(): number {
	try {
		return 1;
	} catch (e) {
		print(e);
		return 2;
	} finally {
		print("fin");
	}
	return 3;
}
