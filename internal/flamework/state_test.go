package flamework

import (
	"path/filepath"
	"reflect"
	"testing"

	"rotor/internal/config"
)

func TestProjectIdentifier_whenGeneratedTwice(t *testing.T) {
	// Given: a controlled obfuscated project salt and one local declaration.
	root := t.TempDir()
	writeProjectFixture(t, root, `{"name":"fixture-game","version":"1.0.0"}`)
	project, err := OpenProject(ProjectOptions{
		ProjectDir: root,
		RootDir:    filepath.Join(root, "src"),
		OutDir:     filepath.Join(root, "out"),
		Config: config.FlameworkConfig{
			Obfuscation:      true,
			IDGenerationMode: string(IDModeObfuscated),
			Salt:             "salt",
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	declaration := DeclarationIdentity{InternalID: "fixture-game:out/services/Foo@Bar", DeclarationName: "Bar", LuaFileName: "foo"}

	// When: the same declaration is resolved twice.
	first, err := project.Identifier(declaration)
	if err != nil {
		t.Fatal(err)
	}
	second, err := project.Identifier(declaration)
	if err != nil {
		t.Fatal(err)
	}

	// Then: exact Hashids output is memoized in build info without persisting configured salt.
	if first != "XG" || second != first {
		t.Fatalf("identifiers = %q, %q, want XG twice", first, second)
	}
	snapshot := project.BuildInfoSnapshot()
	if got := snapshot.Identifiers[declaration.InternalID]; got != "XG" {
		t.Fatalf("stored identifier = %q, want XG", got)
	}
	if snapshot.Salt != nil {
		t.Fatalf("configured salt was persisted as %q", *snapshot.Salt)
	}
}

func TestProjectIdentifier_whenFullModeDoesNotNeedSalt(t *testing.T) {
	// Given: a default full-mode game project with no configured or persisted salt.
	root := t.TempDir()
	writeProjectFixture(t, root, `{"name":"fixture-game","version":"1.0.0"}`)
	project, err := OpenProject(ProjectOptions{ProjectDir: root, RootDir: filepath.Join(root, "src"), OutDir: filepath.Join(root, "out")})
	if err != nil {
		t.Fatal(err)
	}

	// When: a full identifier is generated.
	_, err = project.Identifier(DeclarationIdentity{InternalID: "fixture-game:out/main@Main", DeclarationName: "Main", LuaFileName: "main"})
	// Then: full mode does not create the random salt used only by hashed modes.
	if err != nil {
		t.Fatal(err)
	}
	if salt, ok := project.buildInfo.Salt(); ok {
		t.Fatalf("full mode persisted unused salt %q", salt)
	}
}

func TestProjectPreloadIdentifiers_whenInputOrderVaries(t *testing.T) {
	// Given: unsorted declarations and short-ID mode.
	root := t.TempDir()
	writeProjectFixture(t, root, `{"name":"fixture-game","version":"1.0.0"}`)
	project, err := OpenProject(ProjectOptions{
		ProjectDir: root,
		RootDir:    filepath.Join(root, "src"),
		OutDir:     filepath.Join(root, "out"),
		Config: config.FlameworkConfig{
			IDGenerationMode: string(IDModeTiny),
			Salt:             "salt",
			PreloadIDs:       true,
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	declarations := []DeclarationIdentity{
		{InternalID: "fixture-game:out/z@Z", DeclarationName: "Z", LuaFileName: "z"},
		{InternalID: "fixture-game:out/a@A", DeclarationName: "A", LuaFileName: "a"},
	}

	// When: preload assigns IDs during the serial planning phase.
	got, err := project.PreloadIdentifiers(declarations)
	if err != nil {
		t.Fatal(err)
	}

	// Then: assignment is deterministic by internal identity, independent of discovery order.
	want := map[string]string{
		"fixture-game:out/a@A": "A{XG}",
		"fixture-game:out/z@Z": "Z{dv}",
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("PreloadIdentifiers = %#v, want %#v", got, want)
	}
}

func TestProjectHashString_whenContextRepeats(t *testing.T) {
	// Given: a native project with empty persisted string hashes.
	root := t.TempDir()
	writeProjectFixture(t, root, `{"name":"fixture-game","version":"1.0.0"}`)
	project, err := OpenProject(ProjectOptions{ProjectDir: root, RootDir: filepath.Join(root, "src"), OutDir: filepath.Join(root, "out")})
	if err != nil {
		t.Fatal(err)
	}

	// When: the same context and text are hashed twice.
	first, err := project.HashString("message", "remotes")
	if err != nil {
		t.Fatal(err)
	}
	second, err := project.HashString("message", "remotes")
	if err != nil {
		t.Fatal(err)
	}

	// Then: the UUID is stable and present in the immutable build snapshot.
	if first != second {
		t.Fatalf("hashes = %q and %q", first, second)
	}
	if got := project.BuildInfoSnapshot().StringHashes["remotes:message"]; got != first {
		t.Fatalf("stored string hash = %q, want %q", got, first)
	}
}
