const named = async function named(value: number): Promise<number> {
	return value === 0 ? 0 : named(value - 1);
};
const f = async function different() {};
print(named(2), f);
