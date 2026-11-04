function fact(n: number): number {
	return n <= 1 ? 1 : fact(n - 1);
}

class Singleton {
	static instance(): Singleton {
		return new Singleton();
	}
}

let first = 1, second = first;

const arrow: () => number = () => arrow();
