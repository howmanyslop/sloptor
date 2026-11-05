let i = 10;
function pick(n: number) {
	switch (n) {
		case 0:
		case i++:
			return "low";
		default:
			return "high";
	}
}
print(pick(0), i);
