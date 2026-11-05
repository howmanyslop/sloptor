const bad = math.random() > 2;
if (bad) {
	throw "something went wrong";
}
print("ok");
