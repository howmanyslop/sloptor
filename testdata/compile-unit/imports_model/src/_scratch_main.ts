import greeter, { VALUE, greet as g } from "./_scratch_util";
const x = VALUE + greeter();
print(g("world"), x);
export {};
