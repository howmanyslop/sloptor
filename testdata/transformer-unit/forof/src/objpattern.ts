const objs = [{ x: 1 }, { x: 2 }];
let sum = 0;
for (const { x } of objs) {
	sum += x;
}
print(sum);
