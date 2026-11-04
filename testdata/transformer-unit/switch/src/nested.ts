function grid(a: number, b: number) {
	let r = 0;
	switch (a) {
		case 1:
			switch (b) {
				case 1:
					r = 11;
					break;
				default:
					r = 19;
			}
			break;
		default:
			r = 99;
	}
	return r;
}
print(grid(1, 1), grid(1, 5), grid(2, 0));
