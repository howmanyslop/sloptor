let hits = 0;
for (let i = 0; i !== 3; ) {
	i = i + 1;
	for (let j = 0; j !== 2; j++) {
		if (j === 1) {
			continue;
		}
		hits += 1;
	}
}
print(hits);
