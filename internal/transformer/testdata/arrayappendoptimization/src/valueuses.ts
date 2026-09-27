const result = new Array<number>();
for (const _ of [0]) {
	print(result.push(1), result.push(2));
	result.push(result.push(3));
}
print(result);
