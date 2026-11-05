function describe(n: number): number {
	let result = 0;
	switch (n) {
		case 0:
			let shared = n + 1;
			result = shared;
			break;
		case 1:
			shared = n + 2;
			result = shared;
			break;
	}
	return result;
}
print(describe(0), describe(1));
