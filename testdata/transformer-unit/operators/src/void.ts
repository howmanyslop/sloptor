declare function sideEffect(): number;
declare const flag: boolean;

void sideEffect();
const value = void sideEffect();
const dropped = void flag;
