package flamework

import (
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func TestBuildInfoMarshal_whenFieldsAreAddedOutOfLexicalOrder(t *testing.T) {
	// Given: upstream's constructor order and mutation order.
	info := NewBuildInfo("flamework.build", "1.3.2")
	if err := info.AddIdentifier("z.internal", "id-z"); err != nil {
		t.Fatal(err)
	}
	if err := info.AddIdentifier("a.internal", "id-a"); err != nil {
		t.Fatal(err)
	}
	if err := info.AddClass(BuildClass{
		FilePath:   "out/z.lua",
		InternalID: "class-z",
		Decorators: []BuildDecorator{{Name: "Service", InternalID: "decor-z"}},
	}); err != nil {
		t.Fatal(err)
	}
	info.SetRuntimeConfig(&RuntimeConfig{Profiling: boolPointer(true), LogLevel: runtimeLogLevelPointer(RuntimeLogLevelVerbose)})

	// When: deterministic JSON is prepared repeatedly.
	first, err := info.MarshalOrderedJSON()
	if err != nil {
		t.Fatal(err)
	}
	second, err := info.MarshalOrderedJSON()
	if err != nil {
		t.Fatal(err)
	}

	// Then: bytes match upstream insertion ordering, tabs, and no trailing newline.
	want := "{\n\t\"version\": 1,\n\t\"flameworkVersion\": \"1.3.2\",\n\t\"identifiers\": {\n\t\t\"z.internal\": \"id-z\",\n\t\t\"a.internal\": \"id-a\"\n\t},\n\t\"classes\": [\n\t\t{\n\t\t\t\"filePath\": \"out/z.lua\",\n\t\t\t\"internalId\": \"class-z\",\n\t\t\t\"decorators\": [\n\t\t\t\t{\n\t\t\t\t\t\"name\": \"Service\",\n\t\t\t\t\t\"internalId\": \"decor-z\"\n\t\t\t\t}\n\t\t\t]\n\t\t}\n\t],\n\t\"metadata\": {\n\t\t\"config\": {\n\t\t\t\"logLevel\": \"verbose\",\n\t\t\t\"profiling\": true\n\t\t}\n\t}\n}"
	if string(first) != want {
		t.Fatalf("MarshalOrderedJSON:\n%s\nwant:\n%s", first, want)
	}
	if string(second) != want {
		t.Fatal("repeated marshal changed bytes")
	}
}

func TestLoadBuildInfo_whenInputIsCorrupt(t *testing.T) {
	tests := []struct {
		name string
		data string
		want error
	}{
		{name: "truncated", data: `{"version":1`, want: ErrInvalidBuildInfo},
		{name: "missing required identifiers", data: `{"version":1,"flameworkVersion":"1.3.2"}`, want: ErrInvalidBuildInfo},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Given: a persisted build-info boundary input.
			path := filepath.Join(t.TempDir(), "flamework.build")
			if err := os.WriteFile(path, []byte(tt.data), 0o644); err != nil {
				t.Fatal(err)
			}

			// When: the build info is loaded.
			_, err := LoadBuildInfo(path, "1.3.2")

			// Then: corruption and incompatibility are distinguishable typed failures.
			if !errors.Is(err, tt.want) {
				t.Fatalf("LoadBuildInfo error = %v, want errors.Is(%v)", err, tt.want)
			}
		})
	}
}

func TestBuildInfoPackages_whenFlameworkVersionsDiffer(t *testing.T) {
	// Given: upstream-compatible build info from a different Flamework package version.
	path := filepath.Join(t.TempDir(), "flamework.build")
	input := `{"version":1,"flameworkVersion":"9.9.9","identifiers":{"dep":"id"}}`
	if err := os.WriteFile(path, []byte(input), 0o644); err != nil {
		t.Fatal(err)
	}
	root := NewBuildInfo("root/flamework.build", FlameworkVersion)

	// When: the package build info is loaded and aggregated.
	child, err := LoadBuildInfo(path, FlameworkVersion)
	if err == nil {
		err = root.AddPackage(child)
	}

	// Then: v1.3.2's schema-only version handling preserves the package lookup.
	if err != nil {
		t.Fatalf("aggregate version-mismatched package: %v", err)
	}
	if got, ok := root.Identifier("dep"); !ok || got != "id" {
		t.Fatalf("Identifier dep = %q,%v", got, ok)
	}
}

func TestLoadBuildInfo_whenSchemaPermitsFutureProperties(t *testing.T) {
	// Given: the permissive numeric schema version and partial class accepted by v1.3.2 AJV.
	path := filepath.Join(t.TempDir(), "flamework.build")
	input := `{"version":1.5,"flameworkVersion":"1.3.2","identifiers":{},"future":true,"classes":[{}]}`
	if err := os.WriteFile(path, []byte(input), 0o644); err != nil {
		t.Fatal(err)
	}

	// When: the exact upstream schema boundary is loaded and encoded.
	info, err := LoadBuildInfo(path, "1.3.2")
	if err != nil {
		t.Fatal(err)
	}
	data, err := info.MarshalOrderedJSON()
	if err != nil {
		t.Fatal(err)
	}

	// Then: schema-permitted additions and partial class objects survive.
	for _, fragment := range []string{`"version": 1.5`, `"future": true`, `"classes": [`} {
		if !containsBytes(data, fragment) {
			t.Fatalf("encoded build info omitted %s:\n%s", fragment, data)
		}
	}
}

func TestBuildInfoHashString_whenKeyRepeats(t *testing.T) {
	// Given: an empty persisted string-hash cache.
	info := NewBuildInfo("flamework.build", "1.3.2")

	// When: one context and text pair is requested twice.
	first, err := info.HashString("text", "context")
	if err != nil {
		t.Fatal(err)
	}
	second, err := info.HashString("text", "context")
	if err != nil {
		t.Fatal(err)
	}

	// Then: the UUID is stable and stored under upstream's context-prefixed key.
	if first != second {
		t.Fatalf("hashes = %q then %q", first, second)
	}
	if persisted, ok := info.StringHash("context:text"); !ok || persisted != first {
		t.Fatalf("persisted hash = %q,%v", persisted, ok)
	}
}

func TestBuildInfoPackages_whenNestedPackagesOverlap(t *testing.T) {
	// Given: packages registered in source discovery order, including a grandchild.
	root := NewBuildInfo("root/flamework.build", "1.3.2")
	first := NewBuildInfo("first/flamework.build", "1.3.2")
	first.SetIdentifierPrefix(stringPointer("pkg-first"))
	if err := first.AddIdentifier("shared", "first"); err != nil {
		t.Fatal(err)
	}
	grandchild := NewBuildInfo("grand/flamework.build", "1.3.2")
	grandchild.SetIdentifierPrefix(stringPointer("pkg-grand"))
	if err := grandchild.AddIdentifier("deep", "grand"); err != nil {
		t.Fatal(err)
	}
	if err := first.AddPackage(grandchild); err != nil {
		t.Fatal(err)
	}
	second := NewBuildInfo("second/flamework.build", "1.3.2")
	second.SetIdentifierPrefix(stringPointer("pkg-second"))
	if err := second.AddIdentifier("shared", "second"); err != nil {
		t.Fatal(err)
	}
	if err := root.AddPackage(first); err != nil {
		t.Fatal(err)
	}
	if err := root.AddPackage(second); err != nil {
		t.Fatal(err)
	}

	// When: aggregated lookups and snapshots are requested.
	shared, sharedOK := root.Identifier("shared")
	deep, deepOK := root.Identifier("deep")
	packages := root.PackageSnapshots()

	// Then: first registration wins, recursion works, and snapshots are ordered copies.
	if !sharedOK || shared != "first" || !deepOK || deep != "grand" {
		t.Fatalf("lookups = shared(%q,%v) deep(%q,%v)", shared, sharedOK, deep, deepOK)
	}
	wantPrefixes := []string{"pkg-first", "pkg-grand", "pkg-second"}
	gotPrefixes := make([]string, 0, len(packages))
	for _, snapshot := range packages {
		if prefix, ok := snapshot.IdentifierPrefix(); ok {
			gotPrefixes = append(gotPrefixes, prefix)
		}
	}
	if !reflect.DeepEqual(gotPrefixes, wantPrefixes) {
		t.Fatalf("package prefixes = %v, want %v", gotPrefixes, wantPrefixes)
	}
	packages[0].Identifiers["shared"] = "mutated"
	if got, _ := root.Identifier("shared"); got != "first" {
		t.Fatalf("snapshot mutated aggregate lookup to %q", got)
	}
}

func TestBuildInfoInternalIdentifier_whenIdentifierBelongsToPackage(t *testing.T) {
	// Given: a child identifier aggregated into a root build info.
	root := NewBuildInfo("root/flamework.build", FlameworkVersion)
	child := NewBuildInfo("child/flamework.build", FlameworkVersion)
	if err := child.AddIdentifier("internal", "external"); err != nil {
		t.Fatal(err)
	}
	if err := root.AddPackage(child); err != nil {
		t.Fatal(err)
	}

	// When: reverse lookup crosses the package boundary.
	actualID, actualOK := root.InternalIdentifier("external")
	quirkID, quirkOK := root.InternalIdentifier("internal")

	// Then: v1.3.2's child lookup quirk is preserved exactly.
	if actualOK || actualID != "" {
		t.Fatalf("actual child ID reverse lookup = %q,%v", actualID, actualOK)
	}
	if !quirkOK || quirkID != "external" {
		t.Fatalf("internal-key child lookup = %q,%v", quirkID, quirkOK)
	}
}

func TestBuildInfoOptionalFields_whenEmptyValuesAreExplicit(t *testing.T) {
	// Given: optional fields explicitly present with empty values.
	path := filepath.Join(t.TempDir(), "flamework.build")
	input := `{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"identifierPrefix":"","classes":[],"metadata":{"globs":{"paths":{},"origins":{}}}}`
	if err := os.WriteFile(path, []byte(input), 0o644); err != nil {
		t.Fatal(err)
	}

	// When: it crosses the typed boundary and is encoded.
	info, err := LoadBuildInfo(path, "1.3.2")
	if err != nil {
		t.Fatal(err)
	}
	data, err := info.MarshalOrderedJSON()
	if err != nil {
		t.Fatal(err)
	}

	// Then: explicit empty is not collapsed into omission.
	for _, fragment := range []string{`"identifierPrefix": ""`, `"classes": []`, `"paths": {}`, `"origins": {}`} {
		if !containsBytes(data, fragment) {
			t.Fatalf("encoded build info omitted %s:\n%s", fragment, data)
		}
	}
}

func boolPointer(value bool) *bool                                  { return &value }
func stringPointer(value string) *string                            { return &value }
func runtimeLogLevelPointer(value RuntimeLogLevel) *RuntimeLogLevel { return &value }

func containsBytes(data []byte, fragment string) bool {
	return len(data) >= len(fragment) && stringContains(string(data), fragment)
}

func stringContains(value, fragment string) bool {
	for index := 0; index+len(fragment) <= len(value); index++ {
		if value[index:index+len(fragment)] == fragment {
			return true
		}
	}
	return false
}
