declare function registerOnClose(callback: () => unknown): unknown;

const stopBindToClose = registerOnClose(async function onBindToCloseAsync() {});
print(stopBindToClose);
