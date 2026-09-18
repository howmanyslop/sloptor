import { makeImportedState } from "../../producer/out/contracts";
import type { ImportedState } from "@contract/dependency";
import type { Equal, Expect, NotAny } from "./assertions";

type ExpectedImportedState = { id: string };

type _DependencyStateIsExact = Expect<Equal<ImportedState, ExpectedImportedState>>;
type _DependencyStateIsNotAny = Expect<NotAny<ImportedState>>;
type _SynthesizedImportIsExact = Expect<Equal<typeof makeImportedState, () => ExpectedImportedState>>;
type _SynthesizedImportIsNotAny = Expect<NotAny<typeof makeImportedState>>;

export const imported: ImportedState = makeImportedState();

// @ts-expect-error The public factory must not widen its imported state to any.
export const rejectedFactory: typeof makeImportedState = () => ({ id: 1 });
