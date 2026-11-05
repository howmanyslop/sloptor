interface MethodShape {
	m(n: number): void;
}
interface CallbackShape {
	f: (n: number) => void;
}
const a: MethodShape = {
	m: (n) => {},
};
const b: CallbackShape = {
	f: function (n) {},
};
print(a, b);
