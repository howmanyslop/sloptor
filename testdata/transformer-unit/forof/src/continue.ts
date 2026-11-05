const nums = [1, 2, 3, 4];
let odds = 0;
for (const n of nums) {
	if (n % 2 === 0) {
		continue;
	}
	odds += n;
}
print(odds);
