function pick(kind: string): number {
	switch (kind) {
		case "a":
			return 1;
		default:
			return 0;
		case "b":
			return 2;
	}
}
print(pick("a"), pick("b"), pick("c"));
function fallthrough(kind: string): number {
	let result = 0;
	switch (kind) {
		case "a":
		default:
			result += 10;
		case "b":
			result += 2;
			break;
	}
	return result;
}
print(fallthrough("a"), fallthrough("b"), fallthrough("c"));
let calls = 0;
function label(): number {
	calls++;
	return 2;
}
function effects(kind: number): number {
	switch (kind) {
		case 0:
			return 0;
		default:
			return -1;
		case label():
			return 2;
		case label():
			return 3;
	}
}
print(effects(0), effects(2), calls);
