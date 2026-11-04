const fns: Array<() => number> = [];
for (let i = 0; i < 3; i++) {
	fns[i] = () => i;
}
print(fns[0]());
