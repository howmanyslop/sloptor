declare function multi(): LuaTuple<[number, number]>;
const [d1, d2] = multi();
let e1 = 0;
[e1, , ] = multi();
print(d1, d2, e1);
