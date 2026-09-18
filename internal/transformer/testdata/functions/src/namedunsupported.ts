// Async generators stay rejected: the name would have to bind to both
// TS.async and TS.generator at once.
const asyncGenNamed = async function* asyncGenNamed() {};
print(asyncGenNamed);
