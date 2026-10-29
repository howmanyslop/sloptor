package transformer_test

import (
	"strings"
	"testing"
)

// Under `--!native` Luau only gets a speedup from parameter annotations it
// understands; these cases pin which TS types are annotated and how.
func TestNativeFilesAnnotateSignatures(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")

	for _, want := range []string{
		"--!native\n",
		"local function addVector(a: vector, b: vector): vector\n",
		"local function maybeAddVector(a: vector, b: vector?): vector\n",
		"local function addVector3(a: Vector3, b: Vector3): Vector3\n",
		"local function scale(x: number, factor: number?, label: string?): number\n",
		"local function nameOf(value: string?): string?\n",
		"local function sum(values: { [any]: any }, lookup: { [any]: any }, options: { [any]: any }): number\n",
		"local function sumGeneric(value: number): number\n",
		"local arrow = function(n: number): number\n",
		"function Counter:constructor(start: number)\n",
		"function Counter:next(step: number, other): number\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
}

func TestNativeFilesLeaveUnhelpfulTypesUnannotated(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")

	for _, want := range []string{
		"local function mixed(value, flag, anything): { [any]: any }\n",
		"local function callback(fn): number\n",
		"local loadAsync = TS.async(function(count: number)\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
}

// A bare `table` annotation is tagged userdata by Luau's compiler, which makes
// the native entry guard fail on every call.
func TestNativeFilesNeverEmitBareTable(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/native.ts")
	for _, line := range strings.Split(got, "\n") {
		if strings.Contains(line, ": table") {
			t.Errorf("bare table annotation: %s", line)
		}
	}
}

func TestFilesWithoutNativeDirectiveAreNotAnnotated(t *testing.T) {
	got := renderFunctionsSourceFile(t, "src/nonative.ts")

	for _, want := range []string{
		"local function addVector(a, b)\n",
		"local function scale(x, factor, label)\n",
		"local function sum(values, lookup, options)\n",
		"local arrow = function(n)\n",
		"function Counter:constructor(start)\n",
	} {
		if !strings.Contains(got, want) {
			t.Errorf("missing %q in:\n%s", want, got)
		}
	}
}
