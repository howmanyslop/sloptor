import { Flamework } from "@flamework/core";

class UnsupportedClass {}

export const classGuard = Flamework.createGuard<UnsupportedClass>();
