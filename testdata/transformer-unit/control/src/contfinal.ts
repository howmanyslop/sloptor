let total = 0;
for (let i = 0; i !== 8; i++) {
	if (i === 2) {
		i = i + 1;
		continue;
	}
	total += i;
}
print(total);
