const obj = {
	m(x: number) {
		return x + 1;
	},
	f: function (x: number) {
		return x * 2;
	},
};
print(obj.m(1), obj.f(2));
