declare const nset: Set<number>;
const arr1 = [1, 2];
function takeNums(...nums: Array<number>) {
	return nums.size();
}
print(takeNums(...arr1));
print(takeNums(1, ...arr1));
print(takeNums(...nset));
