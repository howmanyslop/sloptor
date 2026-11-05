declare const flag: boolean;
declare const n: number;
let i = 5;
flag ? print("a") : print("b");
const x = flag ? i++ : 0;
const y = n ? 1 : 2;
for (let j = 0; j < 3; j = flag ? j + 1 : j + 2) {
	print(j);
}
print(i, x, y);
