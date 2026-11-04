declare function gen(): Generator<number, void, unknown>;
for (const n of gen()) {
	print(n);
}
