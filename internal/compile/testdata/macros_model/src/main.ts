// $nameof: trailing property name, and a bare identifier.
print($nameof(player.Humanoid.Health));
print($nameof(foo));

// $keys<T>(): inline the type's string keys as an array.
print($keys<{ x: number; y: string }>());

// $file: a project-relative JSON file -> a Luau table; an importer-relative
// text file -> a Luau string.
print($file("config.json"));
print($file("./notes.txt"));
