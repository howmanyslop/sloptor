package flamework

import (
	"encoding/json"
	"errors"
	"fmt"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func decodeLookupBuild(t *testing.T, fields string) *BuildInfo {
	t.Helper()
	info := NewBuildInfo("fixture/flamework.build", FlameworkVersion)
	if err := info.decode([]byte(`{"version":1,"flameworkVersion":"1.3.2",` + fields + `}`)); err != nil {
		t.Fatal(err)
	}
	return info
}

func TestRootBuildClassLookupSelection(t *testing.T) {
	for _, test := range []struct {
		name, fields, query string
		want                BuildClass
		found               bool
	}{
		{"absent classes", `"identifiers":{}`, "a", BuildClass{}, false},
		{"empty classes", `"identifiers":{},"classes":[]`, "", BuildClass{}, false},
		{"missing", `"identifiers":{},"classes":[{"internalId":"a"}]`, "b", BuildClass{}, false},
		{"zero class is found", `"identifiers":{},"classes":[{}]`, "", BuildClass{}, true},
		{"sorted mapped", `"identifiers":{"key":"b"},"classes":[{"internalId":"a"},{"internalId":"b","filePath":"b.lua","decorators":[]}]`, "key", BuildClass{InternalID: "b", FilePath: "b.lua"}, true},
		{"present empty mapping wins", `"identifiers":{"a":""},"classes":[{"internalId":"","filePath":"empty.lua"},{"internalId":"a"}]`, "a", BuildClass{FilePath: "empty.lua"}, true},
		{"linear fallback", `"identifiers":{},"classes":[{"internalId":"b"},{"internalId":"a","filePath":"a.lua"}]`, "a", BuildClass{InternalID: "a", FilePath: "a.lua"}, true},
		// sort.Search on [b,a,b] selects the third b, not the first linear match.
		{"binary duplicate winner", `"identifiers":{"key":"b"},"classes":[{"internalId":"b","filePath":"first.lua","decorators":[{"name":"First","internalId":"first"}]},{"internalId":"a"},{"internalId":"b","filePath":"third.lua","decorators":[{"name":"Third","internalId":"third"},{"name":"Extra","internalId":"extra"}]}]`, "key", BuildClass{InternalID: "b", FilePath: "third.lua", Decorators: []BuildDecorator{{Name: "Third", InternalID: "third"}, {Name: "Extra", InternalID: "extra"}}}, true},
	} {
		t.Run(test.name, func(t *testing.T) {
			project := &Project{buildInfo: decodeLookupBuild(t, test.fields)}
			before, err := project.buildInfo.MarshalOrderedJSON()
			if err != nil {
				t.Fatal(err)
			}
			got, found := rootBuildClassByInternalID(project, test.query)
			if found != test.found || !reflect.DeepEqual(got, test.want) {
				t.Fatalf("lookup = %#v/%v, want %#v/%v", got, found, test.want, test.found)
			}
			after, err := project.buildInfo.MarshalOrderedJSON()
			if err != nil || string(after) != string(before) {
				t.Fatalf("lookup changed persisted state: %s -> %s (%v)", before, after, err)
			}
		})
	}
	info := NewBuildInfo("fixture/flamework.build", FlameworkVersion)
	err := info.decode([]byte(`{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"classes":null}`))
	if !errors.Is(err, ErrInvalidBuildInfo) || err.Error() != "flamework: invalid flamework.build: classes must not be null" {
		t.Fatalf("persisted null rejection = %v", err)
	}
}

func TestRootBuildClassLookupVisibilityAndOwnership(t *testing.T) {
	info := decodeLookupBuild(t, `"identifiers":{"mapped":"root"},"classes":[{"internalId":"child-key","filePath":"root-collision.lua"},{"internalId":"root","filePath":"root.lua","decorators":[{"name":"Root","internalId":"root-decorator"}]}]`)
	child := decodeLookupBuild(t, `"identifiers":{"child-key":"child-target","mapped":"child-target"},"classes":[{"internalId":"child-target","filePath":"child.lua"},{"internalId":"root","filePath":"child-collision.lua"}]`)
	grandchild := decodeLookupBuild(t, `"identifiers":{"grand-key":"grand-target"},"classes":[{"internalId":"grand-target"}]`)
	child.packages = []*BuildInfo{grandchild}
	info.packages = []*BuildInfo{child}
	project := &Project{buildInfo: info}
	for query, path := range map[string]string{"child-key": "root-collision.lua", "root": "root.lua", "mapped": "root.lua"} {
		if got, found := rootBuildClassByInternalID(project, query); !found || got.FilePath != path {
			t.Fatalf("root lookup %q = %#v/%v", query, got, found)
		}
	}
	for _, query := range []string{"child-target", "grand-key", "grand-target"} {
		if got, found := rootBuildClassByInternalID(project, query); found {
			t.Fatalf("child-only lookup %q escaped root: %#v", query, got)
		}
	}
	previous, _ := rootBuildClassByInternalID(project, "mapped")
	returned, _ := rootBuildClassByInternalID(project, "mapped")
	returned.Decorators[0].Name = "returned mutation"
	if previous.Decorators[0].Name != "Root" || (*info.classes)[1].Decorators[0].Name != "Root" {
		t.Fatal("returned decorators alias the root or another result")
	}
	(*info.classes)[1].Decorators[0].Name = "current"
	got, _ := rootBuildClassByInternalID(project, "mapped")
	if got.Decorators[0].Name != "current" || previous.Decorators[0].Name != "Root" {
		t.Fatal("lookup is stale or root mutation changed a previous result")
	}
	info.identifiers.values["mapped"] = "child-key"
	if got, _ := rootBuildClassByInternalID(project, "mapped"); got.FilePath != "root-collision.lua" {
		t.Fatalf("mapping mutation ignored: %#v", got)
	}
	delete(info.identifiers.values, "mapped")
	if _, found := rootBuildClassByInternalID(project, "mapped"); found {
		t.Fatal("deleted mapping was retained")
	}
	other := &Project{buildInfo: decodeLookupBuild(t, `"identifiers":{"mapped":"root"},"classes":[{"internalId":"root","filePath":"other.lua"}]`)}
	replacement := []BuildClass{{InternalID: "root", FilePath: "replacement.lua"}}
	info.classes = &replacement
	if got, _ := rootBuildClassByInternalID(project, "root"); got.FilePath != "replacement.lua" {
		t.Fatalf("class replacement ignored: %#v", got)
	}
	if got, _ := rootBuildClassByInternalID(other, "mapped"); got.FilePath != "other.lua" {
		t.Fatalf("same-path Projects shared state: %#v", got)
	}
}

func TestRootBuildClassLookupEmptyAndNilContract(t *testing.T) {
	var nilClasses []BuildClass
	emptyClasses := []BuildClass{}
	for _, classes := range []*[]BuildClass{nil, &nilClasses, &emptyClasses} {
		if got, found := rootBuildClassByInternalID(&Project{buildInfo: &BuildInfo{classes: classes}}, ""); found || !reflect.DeepEqual(got, BuildClass{}) {
			t.Fatalf("empty representation = %#v/%v", got, found)
		}
	}
	for _, broken := range []*Project{nil, {}} {
		capture := func(lookup func(*Project, string) (BuildClass, bool)) (result string) {
			defer func() {
				if value := recover(); value != nil {
					result = fmt.Sprintf("%T:%v", value, value)
				}
			}()
			lookup(broken, "")
			return ""
		}
		old := capture(func(project *Project, id string) (BuildClass, bool) {
			snapshot := project.BuildInfoSnapshot()
			return buildClassByInternalID(snapshot, id)
		})
		if got := capture(rootBuildClassByInternalID); old == "" || got != old {
			t.Fatalf("nil contract = %q, want %q", got, old)
		}
	}
}

func TestRootBuildClassLookupIgnoresUnrelatedMetadata(t *testing.T) {
	info := decodeLookupBuild(t, `"identifiers":{"key":"a"},"metadata":{"config":{"logLevel":"verbose","extra":{"nested":[1,2]}},"globs":{"paths":{"pattern":["a.ts","b.ts"]}}},"classes":[{"internalId":"a","decorators":[]}]`)
	project := &Project{buildInfo: info}
	// Snapshot silently omits invalid in-memory runtime config. The lookup must
	// still return the selected class without changing or validating that config.
	info.metadata.Config.additional["invalid"] = json.RawMessage("{")
	level := RuntimeLogLevel("invalid")
	info.metadata.Config.LogLevel = &level
	if got, found := rootBuildClassByInternalID(project, "key"); !found || !reflect.DeepEqual(got, BuildClass{InternalID: "a"}) {
		t.Fatalf("lookup with unrelated invalid config = %#v/%v", got, found)
	}
	if string(info.metadata.Config.additional["invalid"]) != "{" || *info.metadata.Config.LogLevel != level {
		t.Fatal("lookup changed unrelated runtime config")
	}
}

func TestDiscoverFlameworkClassesDecodedLookupPayload(t *testing.T) {
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-package","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
	writeTransformFixture(t, directory, "src/service.ts", `export class Service {}`)
	input := newClassAnalysisInput(t, directory, "src/service.ts")
	info := decodeLookupBuild(t, `"identifiers":{"fixture-package:out/service@Service":"b"},"classes":[{"internalId":"b","filePath":"first.lua","decorators":[{"name":"First","internalId":"first"}]},{"internalId":"a"},{"internalId":"b","filePath":"third.lua","decorators":[{"name":"Third","internalId":"third"},{"name":"Extra","internalId":"extra"}]}]`)
	input.Project.buildInfo = info
	before, err := info.MarshalOrderedJSON()
	if err != nil {
		t.Fatal(err)
	}
	for attempt := 0; attempt < 2; attempt++ {
		analysis, err := AnalyzeFlameworkClasses(input)
		if err != nil {
			t.Fatal(err)
		}
		files := analysis.Files()
		if len(files) != 1 || len(files[0].Classes) != 1 {
			t.Fatalf("analysis = %#v", files)
		}
		plan := files[0].Classes[0]
		want := []DecoratorPlan{{Name: "Third", InternalID: "third"}, {Name: "Extra", InternalID: "extra"}}
		if !reflect.DeepEqual(plan.Decorators, want) || plan.containsLegacyDecorator {
			t.Fatalf("persisted decorators/provenance = %#v", plan)
		}
		if got := analysis.Classes()[0].DecoratorIDs; !reflect.DeepEqual(got, []string{"third", "extra"}) {
			t.Fatalf("discovered decorator IDs = %v", got)
		}
		plan.Decorators[0].Name = "returned mutation"
	}
	after, err := info.MarshalOrderedJSON()
	if err != nil || string(after) != string(before) {
		t.Fatalf("fallback changed class/identifier insertion order: %s -> %s (%v)", before, after, err)
	}
}

func TestDiscoverFlameworkClassesLookupFailureBoundaries(t *testing.T) {
	for _, test := range []struct {
		name, source, packageJSON, want string
	}{
		{"decorator before lookup", `import { Decorator } from "external"; @Decorator() export class Service {}`, `{"types":"index.d.ts"}`, "invalid Flamework package.json: declaration package"},
		{"constructor after lookup", `export class Service<T extends string> { constructor(value: T extends string ? 1 : 2) {} }`, `{"name":"external","types":"index.d.ts"}`, `Could not find UID for type "T extends string ? 1 : 2"`},
	} {
		t.Run(test.name, func(t *testing.T) {
			directory := t.TempDir()
			writeTransformFixture(t, directory, "package.json", `{"name":"fixture-package"}`)
			writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"experimentalDecorators":true,"moduleResolution":"node","rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
			writeTransformFixture(t, directory, "src/service.ts", test.source)
			writeTransformFixture(t, directory, "node_modules/external/package.json", test.packageJSON)
			writeTransformFixture(t, directory, "node_modules/external/index.d.ts", `export declare function Decorator(): { (target: unknown): void; _flamework_Decorator: "Class" };`)
			writeTransformFixture(t, directory, "flamework.build", `{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"classes":[{"internalId":"fixture-package:out/service@Service"}]}`)
			input := newClassAnalysisInput(t, directory, "src/service.ts")
			before, err := input.Project.buildInfo.MarshalOrderedJSON()
			if err != nil {
				t.Fatal(err)
			}
			_, err = AnalyzeFlameworkClasses(input)
			if err == nil || !strings.Contains(err.Error(), test.want) {
				t.Fatalf("analysis error = %v, want %q", err, test.want)
			}
			after, marshalErr := input.Project.buildInfo.MarshalOrderedJSON()
			if marshalErr != nil || string(after) != string(before) {
				t.Fatalf("failed discovery changed metadata: %s -> %s (%v)", before, after, marshalErr)
			}
		})
	}
}

func TestDiscoverFlameworkClassesPackageRootLookup(t *testing.T) {
	for _, local := range []bool{false, true} {
		t.Run(fmt.Sprintf("local=%v", local), func(t *testing.T) {
			directory := t.TempDir()
			writeTransformFixture(t, directory, "package.json", `{"name":"fixture-package"}`)
			writeTransformFixture(t, directory, "flamework.build", `{"version":1,"flameworkVersion":"1.3.2","identifiers":{"fixture-package:project/out/service@Service":"parent"},"classes":[{"internalId":"parent","decorators":[{"name":"Parent","internalId":"parent-decorator"}]}]}`)
			projectDirectory := filepath.Join(directory, "project")
			writeTransformFixture(t, projectDirectory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out"},"files":["src/service.ts"]}`)
			writeTransformFixture(t, projectDirectory, "src/service.ts", `export class Service {}`)
			if local {
				// A present empty project file takes precedence over package-root data.
				writeTransformFixture(t, projectDirectory, "flamework.build", `{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"classes":[]}`)
			}
			input := newClassAnalysisInput(t, projectDirectory, "src/service.ts")
			analysis, err := AnalyzeFlameworkClasses(input)
			if err != nil {
				t.Fatal(err)
			}
			if local {
				if len(analysis.Classes()) != 0 {
					t.Fatalf("local empty metadata leaked package fallback: %#v", analysis.Classes())
				}
				return
			}
			if got := analysis.Classes(); len(got) != 1 || !reflect.DeepEqual(got[0].DecoratorIDs, []string{"parent-decorator"}) {
				t.Fatalf("package-root fallback = %#v", got)
			}
			if plan := analysis.Files()[0].Classes[0]; plan.containsLegacyDecorator || plan.Decorators[0].Name != "Parent" {
				t.Fatalf("package-root payload/provenance = %#v", plan)
			}
		})
	}
}
