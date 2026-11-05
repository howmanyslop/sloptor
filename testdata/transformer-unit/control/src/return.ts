function nestedReturn(value: number) {
	{
		if (value > 0) {
			return;
		}
		print("nested");
	}
	print(value);
}
nestedReturn(1);
