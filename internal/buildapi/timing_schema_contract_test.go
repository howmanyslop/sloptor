package buildapi

import (
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"runtime"
	"slices"
	"strings"
	"testing"

	"rotor/internal/compile"
)

func TestProjectTimingSchemaMatchesJavaScriptAndTypeScript(t *testing.T) {
	repoRoot := timingSchemaRepoRoot(t)
	javascript := readTimingSchemaFile(t, filepath.Join(repoRoot, "api.js"))
	typescript := readTimingSchemaFile(t, filepath.Join(repoRoot, "api.d.ts"))

	goStages, goOptionalStages := goTimingFields(reflect.TypeFor[compile.BuildTimingStages]())
	jsStages := javascriptTimingFields(t, javascript, "REQUIRED_TIMING_STAGES")
	tsStages, tsOptionalStages := typescriptTimingFields(t, typescript, "BuildTimingStages")
	assertTimingFields(t, "required stages in Go and JavaScript", goStages, jsStages)
	assertTimingFields(t, "required stages in Go and TypeScript", goStages, tsStages)
	assertTimingFields(t, "optional stages in Go and TypeScript", goOptionalStages, tsOptionalStages)

	goCounts, goOptionalCounts := goTimingFields(reflect.TypeFor[compile.BuildTimingCounts]())
	jsCounts := javascriptTimingFields(t, javascript, "REQUIRED_TIMING_COUNTS")
	tsCounts, tsOptionalCounts := typescriptTimingFields(t, typescript, "BuildTimingCounts")
	assertTimingFields(t, "required counts in Go and JavaScript", goCounts, jsCounts)
	assertTimingFields(t, "required counts in Go and TypeScript", goCounts, tsCounts)
	assertTimingFields(t, "optional counts in Go and TypeScript", goOptionalCounts, tsOptionalCounts)
}

func timingSchemaRepoRoot(t *testing.T) string {
	t.Helper()
	_, fileName, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("locate timing schema contract test")
	}
	return filepath.Clean(filepath.Join(filepath.Dir(fileName), "..", ".."))
}

func readTimingSchemaFile(t *testing.T, path string) string {
	t.Helper()
	contents, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	return string(contents)
}

func goTimingFields(schema reflect.Type) (required, optional []string) {
	for index := range schema.NumField() {
		jsonTag := strings.Split(schema.Field(index).Tag.Get("json"), ",")
		if jsonTag[0] == "" || jsonTag[0] == "-" {
			continue
		}
		if slices.Contains(jsonTag[1:], "omitempty") {
			optional = append(optional, jsonTag[0])
		} else {
			required = append(required, jsonTag[0])
		}
	}
	slices.Sort(required)
	slices.Sort(optional)
	return required, optional
}

func javascriptTimingFields(t *testing.T, source, constant string) []string {
	t.Helper()
	arrayPattern := regexp.MustCompile(`(?s)const\s+` + regexp.QuoteMeta(constant) + `\s*=\s*\[(.*?)\];`)
	match := arrayPattern.FindStringSubmatch(source)
	if match == nil {
		t.Fatalf("JavaScript timing schema has no %s array", constant)
	}
	fieldPattern := regexp.MustCompile(`"([A-Za-z][A-Za-z0-9]*)"`)
	fields := []string{}
	for _, field := range fieldPattern.FindAllStringSubmatch(match[1], -1) {
		fields = append(fields, field[1])
	}
	slices.Sort(fields)
	return fields
}

func typescriptTimingFields(t *testing.T, source, interfaceName string) (required, optional []string) {
	t.Helper()
	interfacePattern := regexp.MustCompile(`(?s)export\s+interface\s+` + regexp.QuoteMeta(interfaceName) + `\s*\{(.*?)\}`)
	match := interfacePattern.FindStringSubmatch(source)
	if match == nil {
		t.Fatalf("TypeScript timing schema has no %s interface", interfaceName)
	}
	fieldPattern := regexp.MustCompile(`readonly\s+([A-Za-z][A-Za-z0-9]*)(\?)?\s*:\s*number\s*;`)
	for _, field := range fieldPattern.FindAllStringSubmatch(match[1], -1) {
		if field[2] == "?" {
			optional = append(optional, field[1])
		} else {
			required = append(required, field[1])
		}
	}
	slices.Sort(required)
	slices.Sort(optional)
	return required, optional
}

func assertTimingFields(t *testing.T, name string, left, right []string) {
	t.Helper()
	if !slices.Equal(left, right) {
		t.Errorf("%s differ:\nGo/left: %v\nclient/right: %v", name, left, right)
	}
}
