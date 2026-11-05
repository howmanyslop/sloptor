export function f5(): number {
	try {
		return 1;
	} catch {
		return 2;
	}
	return 3;
}
export function f6() {
	for (let i = 0; i < 10; i++) {
		try {
			if (i === 5) {
				break;
			}
			print(i);
		} catch {
			continue;
		}
	}
}
export function f7(): number {
	while (true) {
		try {
			return 42;
		} catch {
			break;
		}
	}
	return 0;
}
export function f8(): number {
	try {
		try {
			return 1;
		} catch {}
	} catch {}
	return 2;
}
export function sw(v: number) {
	switch (v) {
		case 1:
			try {
				break;
			} catch {}
		default:
			print("d");
	}
}
export function contOnly() {
	for (let i = 0; i < 3; i++) {
		try {
			continue;
		} catch {}
	}
}
export function finReturn(): number {
	try {
		print("t");
	} finally {
		return 9;
	}
}
