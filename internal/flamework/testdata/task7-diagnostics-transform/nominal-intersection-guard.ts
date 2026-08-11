import { Flamework } from "@flamework/core";

type Impossible = Part & Folder;

Flamework.createGuard<Impossible>();
