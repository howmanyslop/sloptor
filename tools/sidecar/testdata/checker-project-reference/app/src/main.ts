import type { ReferenceContract } from "../../shared/src/value";

declare const reference: ReferenceContract;

export const probe = reference.origin;
export const checkerProbe = "CHECKER_REFERENCE_VALUE";
