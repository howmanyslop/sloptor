function over(a: number): number;
function over(a: string): number;
function over(a: number | string) {
	return 1;
}
print(over(1), over("x"));
