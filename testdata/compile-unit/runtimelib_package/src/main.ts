declare function print(...params: Array<unknown>): void;
declare class Foo {}
declare const inst: object;
const isFoo = inst instanceof Foo;
print(isFoo);
