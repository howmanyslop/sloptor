interface Obj {
	method(this: Obj): number;
	callback: (x: number) => number;
	then(this: Obj): number;
}
declare const obj: Obj;
obj.method();
obj.callback(1);
obj.then();
interface Lookup {
	["a b"]: (x: number) => number;
}
declare const lookup: Lookup;
lookup["a b"](5);
const arr = [10, 20, 30];
arr[0] += 1;
let i = 2;
arr[i] *= 2;
const prev = arr[i - 1];
arr[i]++;
interface MutObj {
	cb: (x: number) => number;
}
declare let mutObj: MutObj;
mutObj.cb(i++);
print(obj, lookup, arr, prev, i, mutObj);
