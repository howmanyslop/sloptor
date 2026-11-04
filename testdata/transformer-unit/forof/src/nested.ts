const grid = [[1, 2], [3, 4]];
let sum = 0;
for (const row of grid) {
	for (const cell of row) {
		sum += cell;
	}
}
print(sum);
