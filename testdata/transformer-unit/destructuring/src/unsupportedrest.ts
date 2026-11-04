declare const values: IterableFunction<number>;
const [...bindingRest] = values;
let assignmentRest = new Array<number>();
[, ...assignmentRest] = values;
