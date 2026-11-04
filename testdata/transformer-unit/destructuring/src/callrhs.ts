function make(): Array<number> {
	return [1, 2];
}
const [p, q] = make();
let m = 0;
let n = 0;
[m, n] = make();
print(p, q, m, n);
