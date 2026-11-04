export function early() {
	return lateAsync();
}
async function lateAsync() {
	return 1;
}
class Foo {
	async work(n: number) {
		return (await Promise.resolve(n)) + 1;
	}
	static async make() {
		return new Foo();
	}
	*counter() {
		yield 1;
		yield 2;
	}
	static *names() {
		yield "a";
	}
	async ["computed " + "key"]() {
		return 1;
	}
}
export const obj = {
	async work(n: number) {
		return (await Promise.resolve(n)) + 1;
	},
	*counter() {
		yield 1;
	},
};
export function use() {
	const f = new Foo();
	print(f.work(1), Foo.make(), f.counter(), Foo.names());
}
