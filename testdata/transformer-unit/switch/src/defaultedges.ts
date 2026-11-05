function order(kind: number): number {
	let log = 0;
	function label(value: number): number {
		log = log * 10 + value;
		return value;
	}
	switch (kind) {
		case label(1):
			log = log * 10 + 2;
			break;
		default:
			log = log * 10 + 3;
		case label(2):
			break;
	}
	return log;
}
print(order(1), order(2), order(3));
function prereqs(kind: number): number {
	let i = 1;
	switch (kind) {
		default:
			return i;
		case i++:
			return i;
		case i++:
			return i;
	}
}
print(prereqs(1), prereqs(9));
function first(kind: number): number {
	let result = 0;
	switch (kind) {
		default:
			result += 1;
			break;
		case 1:
			result += 8;
	}
	return result;
}
print(first(0), first(1));
function hoist(kind: number): number {
	let result = 0;
	switch (kind) {
		case 0:
			let shared = 10;
		default:
			shared = 11;
			result = shared;
			break;
		case 1:
			shared = 20;
			result = shared;
			break;
	}
	return result;
}
print(hoist(0), hoist(1));

function defaultHoist(kind: number): number {
	let result = 0;
	let read = () => 0;
	switch (kind) {
		default:
			let shared = 10;
			read = () => shared;
		case 1:
			shared = 11;
			result = read();
			break;
	}
	return result;
}
print(defaultHoist(0));
