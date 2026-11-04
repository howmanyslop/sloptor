declare const external: unique symbol;
/** @metadata {@link external intrinsic-const} */
declare function inspect(value: object): void;

inspect({ valid: true });

export {};
