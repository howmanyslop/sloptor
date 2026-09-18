export type Equal<Left, Right> = (<Value>() => Value extends Left ? 1 : 2) extends <Value>() => Value extends Right ? 1 : 2
  ? (<Value>() => Value extends Right ? 1 : 2) extends <Value>() => Value extends Left ? 1 : 2
    ? true
    : false
  : false;

export type NotAny<Value> = 0 extends 1 & Value ? false : true;

export type Expect<Condition extends true> = Condition;
