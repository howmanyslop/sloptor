declare const flag: boolean;

const short = flag && async function shortNamed() {
	return 1;
};

const ternary = flag
	? async function thenNamed() {
			return 1;
		}
	: async function elseNamed() {
			return 2;
		};

print(short, ternary);
