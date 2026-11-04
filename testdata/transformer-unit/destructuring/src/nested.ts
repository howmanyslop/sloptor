const data = { list: [1, 2], info: { name: "n" } };
const {
	list: [u, v],
	info: { name },
} = data;
const records = [{ id: 1 }, { id: 2 }];
const [{ id: firstId }] = records;
print(u, v, name, firstId);
