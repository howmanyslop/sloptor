function make(n: number) {
	return [n, n + 1];
}
let i = 0;
let sum = 0;
for (const v of make(i++)) {
	sum += v;
}
print(sum, i);
