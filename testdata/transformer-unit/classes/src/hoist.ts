function makeInstance() {
	return new Late();
}

class Late {
	tag = "late";
}

print(makeInstance());
