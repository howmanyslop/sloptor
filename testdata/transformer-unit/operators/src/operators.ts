declare let obj: { [key: string]: number };
declare let key: string;
declare let flags: number;
declare let mask: number;
declare let num: number;
declare let maybeBool: boolean | undefined;
declare let str: string;
declare class Foo {}
declare let inst: Foo;

const hasKey = key in obj;
flags &= mask;
const notNum = !num;
const coalesced = maybeBool ?? true;
str += num;
const isFoo = inst instanceof Foo;
const fresh = false;
const coalesced2 = fresh ?? true;
let cond: boolean | undefined;
const coalesced3 = cond ?? false;
const inverted = ~num;
num **= mask;
num >>= mask;
