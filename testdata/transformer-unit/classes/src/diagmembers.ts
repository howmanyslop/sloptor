class BadMembers {
	new() {}
	__add() {}
	get value(): number {
		return 1;
	}
	set value(v: number) {}
}

class Collides {
	m() {}
	static m() {}
}

print(new BadMembers(), new Collides());
