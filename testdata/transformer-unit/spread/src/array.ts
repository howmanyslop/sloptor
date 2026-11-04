declare const nset: Set<number>;
declare const nmap: Map<string, number>;
declare const str: string;
function getObj() {
	return { b: 1 };
}
const arr1 = [1, 2];
const arr2 = [3, 4];
const a1 = [...arr1];
const a2 = [...arr1, 5];
const a3 = [5, ...arr1];
const a4 = [...arr1, ...arr2];
const a5 = [...arr1, 5, ...arr2, 6];
const a6 = [...nset];
const a7 = [...nmap];
const a8 = [...str];
const a9 = [...arr1, getObj().b];
print(a1, a2, a3, a4, a5, a6, a7, a8, a9);
