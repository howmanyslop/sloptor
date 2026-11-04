class Secretive {
	#hidden = 1;
	#peek() {
		return this.#hidden;
	}
}

print(new Secretive());
