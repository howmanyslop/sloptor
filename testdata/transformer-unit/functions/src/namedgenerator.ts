const named = function* named() {
	yield 1;
};
const g = function* generatorNamed() {
	yield 2;
};
print(named(), g());
