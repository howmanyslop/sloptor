let b = 1;
const source = [] as Array<number>;
const [a = b++] = source;
const { m: c = b++ } = {} as { m?: number };
print(a, b, c);
