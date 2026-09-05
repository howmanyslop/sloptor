import Dependency from "@contract/dependency";

export interface OrderedMembers {
  trailing: string;
  leading: number;
}

export type OrderedUnion =
  | { kind: "string"; value: string }
  | { kind: "number"; value: number };

export type SpringConfiguration = {
  stiffness: number;
};

export type AnimationConfiguration = SpringConfiguration;

export const configuration: AnimationConfiguration = { stiffness: 1 };

export const makeImportedState = () => Dependency.createState();

export const pair: [import("@contract/dependency").ImportedState, AnimationConfiguration] = [
  makeImportedState(),
  configuration,
];
