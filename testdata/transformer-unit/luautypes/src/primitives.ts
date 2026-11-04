// Every primitive, top and bottom type, literal, and Roblox name the builder
// spells on its own. Declared values keep the fixture free of runtime code.
export declare const aNumber: number;
export declare const aString: string;
export declare const aBoolean: boolean;
export declare const anAny: any;
export declare const anUnknown: unknown;
export declare const aNever: never;
export declare const anUndefined: undefined;
export declare const aNull: null;
export declare const aVoid: void;
export declare const aBigInt: bigint;
export declare const aSymbol: symbol;
export declare const anObject: object;
export declare const anEmpty: {};

export declare const stringLiteral: "x";
export declare const trueLiteral: true;
export declare const falseLiteral: false;
export declare const numberLiteral: 1;
export declare const template: `id-${string}`;
export declare const escaped: "a\"b\\c\n";

export declare const aVector: vector;
export declare const aVector3: Vector3;
export declare const aCFrame: CFrame;
export declare const aPart: Part;
export declare const anInstance: Instance;
export declare const aMaterial: Enum.Material;
export declare const plastic: Enum.Material.Plastic;
export declare const anEnumItem: EnumItem;
export declare const aSignal: RBXScriptSignal;

export const config = { depth: 1 };
