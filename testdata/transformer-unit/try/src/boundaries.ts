declare function getTuple(): LuaTuple<[number, string]>;
export function multi(): LuaTuple<[number, string]> {
	try {
		return $tuple(1, "a");
	} catch {}
	return getTuple();
}
export function tupleVal(): LuaTuple<[number, string]> {
	try {
		return getTuple();
	} catch {}
	return getTuple();
}
export function bareReturn() {
	try {
		if (math.random() > 0.5) {
			return;
		}
		print("x");
	} catch {}
}
export async function tryInAsync(): Promise<number> {
	try {
		return await Promise.resolve(1);
	} catch (e) {
		return 2;
	}
}
export function loopInTry() {
	try {
		for (let i = 0; i < 3; i++) {
			if (i === 1) {
				break;
			}
			print(i);
		}
	} catch {}
}
export function fnInTry() {
	try {
		const f = () => {
			return 5;
		};
		print(f());
	} catch {}
}
