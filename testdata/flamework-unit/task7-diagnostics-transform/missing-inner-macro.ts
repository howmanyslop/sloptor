import { Modding } from "@flamework/core";

type MissingGenericInputs = { _flamework_macro_generic: {} };

Modding.inspect<Modding.Intrinsic<"obfuscate-obj", [MissingGenericInputs, "remotes"]>>();
