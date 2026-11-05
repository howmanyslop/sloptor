let total = 0;
for (let i = 0; i < 5; i++) {
	switch (i % 3) {
		case 0:
			total += 1;
			break;
		case 1:
			continue;
		default:
			total += 10;
	}
	total += 100;
}
print(total);
