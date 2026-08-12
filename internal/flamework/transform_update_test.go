package flamework

import (
	"context"
	"errors"
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/config"
	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/printer"
	"rotor/tsgo/tspath"
)

func TestTransformFlameworkExpression_rewritesCompoundAttributeAssignment_whenMetadataIsInherited(t *testing.T) {
	// Given
	state, sourceFile := newExpressionTransformFixture(t, `
import { BaseComponent as RenamedComponent } from "./base";
interface Attributes { count: number }
declare function next(): number;
class Counter extends RenamedComponent<Attributes> {
	update() { this.attributes.count += next(); }
}
`)
	expression := findExpressionKind(t, sourceFile.AsNode(), ast.KindBinaryExpression)

	// When
	transformed, err := transformFlameworkExpression(state, expression)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkExpression() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(transformed, sourceFile)
	want := `this[SYMBOL_ATTRIBUTE_SETTER]("count", this.attributes.count + next())`
	if strings.TrimSuffix(printed, ";") != want {
		t.Fatalf("transformed expression = %q, want %q", printed, want)
	}
	t.Logf("transformed TypeScript: %s", printed)
}

func TestTransformFlameworkExpressionsInSourceFile_preservesAttributeUpdateForms_whenNestedInMethods(t *testing.T) {
	// Given
	state, sourceFile := newExpressionTransformFixture(t, `
import { BaseComponent } from "./base";
interface Attributes { count: number; [key: string]: number }
declare function next(): number;
class Counter extends BaseComponent<Attributes> {
	set(key: string) { this.attributes[key] = next(); }
	prefix() { return ++this.attributes.count; }
	postfix() { return this.attributes.count++; }
	remove() { delete this.attributes.count; }
}
`)

	// When
	transformed, err := transformFlameworkExpressionsInSourceFile(state, sourceFile)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkExpressionsInSourceFile() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(transformed)
	wants := []string{
		`import { SYMBOL_ATTRIBUTE_SETTER } from "@flamework/components/out/baseComponent";`,
		`this[SYMBOL_ATTRIBUTE_SETTER](key, next());`,
		`this[SYMBOL_ATTRIBUTE_SETTER]("count", this.attributes.count + 1)`,
		`this[SYMBOL_ATTRIBUTE_SETTER]("count", this.attributes.count + 1, true)`,
		`this[SYMBOL_ATTRIBUTE_SETTER]("count", undefined);`,
	}
	for _, want := range wants {
		if !strings.Contains(printed, want) {
			t.Fatalf("transformed source missing %q:\n%s", want, printed)
		}
	}
	reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: "/reparsed.ts",
		Path:     tspath.Path("/reparsed.ts"),
	}, printed, core.ScriptKindTS)
	if len(reparsed.Diagnostics()) != 0 {
		t.Fatalf("reparse diagnostics = %v", reparsed.Diagnostics())
	}
	t.Logf("transformed TypeScript:\n%s", printed)
}

func TestTransformFlameworkExpression_rewritesKnownNetworkingKey_whenObfuscationMarkerIsPresent(t *testing.T) {
	// Given
	state, sourceFile := newExpressionTransformFixture(t, `
declare const events: {
	readonly _flamework_key_obfuscation: "remotes";
	readonly submitted: () => void;
};
events.submitted;
`)
	expression := findExpressionKind(t, sourceFile.AsNode(), ast.KindPropertyAccessExpression)

	// When
	transformed, err := transformFlameworkExpression(state, expression)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkExpression() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(transformed, sourceFile)
	want := `events["submitted" as "submitted"]`
	if strings.TrimSuffix(printed, ";") != want {
		t.Fatalf("transformed expression = %q, want %q", printed, want)
	}
	t.Logf("transformed TypeScript: %s", printed)
}

func TestTransformFlameworkExpression_persistsNetworkingKeyHash_whenObfuscationIsEnabled(t *testing.T) {
	// Given
	state, sourceFile := newExpressionTransformFixture(t, `
declare const events: {
	readonly _flamework_key_obfuscation: "remotes";
	readonly submitted: () => void;
};
events.submitted;
`)
	state.project.config.Obfuscation = true
	expression := findExpressionKind(t, sourceFile.AsNode(), ast.KindPropertyAccessExpression)

	// When
	transformed, err := transformFlameworkExpression(state, expression)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkExpression() error = %v", err)
	}
	hash := state.project.BuildInfoSnapshot().StringHashes["remotes:submitted"]
	if hash == "" {
		t.Fatal("networking key hash was not persisted")
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(transformed, sourceFile)
	want := `events["` + hash + `" as "submitted"]`
	if strings.TrimSuffix(printed, ";") != want {
		t.Fatalf("transformed expression = %q, want %q", printed, want)
	}
	t.Logf("transformed TypeScript: %s", printed)
}

func TestTransformFlameworkExpression_evaluatesAttributeReceiverOnce_whenUsingDirectAssignment(t *testing.T) {
	// Given
	state, sourceFile := newExpressionTransformFixture(t, `
import { BaseComponent } from "./base";
interface Attributes { count: number }
class Counter extends BaseComponent<Attributes> {}
declare function getCounter(): Counter;
declare function next(): number;
getCounter().attributes.count = next();
`)
	expression := findExpressionKind(t, sourceFile.AsNode(), ast.KindBinaryExpression)

	// When
	transformed, err := transformFlameworkExpression(state, expression)
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkExpression() error = %v", err)
	}
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(transformed, sourceFile)
	want := `getCounter()[SYMBOL_ATTRIBUTE_SETTER]("count", next())`
	if strings.TrimSuffix(printed, ";") != want {
		t.Fatalf("transformed expression = %q, want %q", printed, want)
	}
	if strings.Count(printed, "getCounter()") != 1 {
		t.Fatalf("receiver evaluation count = %d, want 1: %s", strings.Count(printed, "getCounter()"), printed)
	}
	t.Logf("transformed TypeScript: %s", printed)
}

func TestTransformFlameworkExpression_rejectsDynamicNetworkingKey_whenObfuscationMarkerIsPresent(t *testing.T) {
	// Given
	state, sourceFile := newExpressionTransformFixture(t, `
declare const events: {
	readonly _flamework_key_obfuscation: "remotes";
	readonly [key: string]: unknown;
};
declare const key: string;
events[key];
`)
	expression := findExpressionKind(t, sourceFile.AsNode(), ast.KindElementAccessExpression)

	// When
	_, err := transformFlameworkExpression(state, expression)

	// Then
	if !errors.Is(err, ErrDynamicObfuscatedAccess) {
		t.Fatalf("transformFlameworkExpression() error = %v, want ErrDynamicObfuscatedAccess", err)
	}
}

func newExpressionTransformFixture(t *testing.T, source string) (*TransformState, *ast.SourceFile) {
	t.Helper()
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"strict":true,"rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}`)
	writeTransformFixture(t, directory, "src/base.ts", `
export class BaseComponent<A> {
	/** @metadata intrinsic-component-attributes */
	readonly attributes!: A;
}
`)
	writeTransformFixture(t, directory, "src/expression.ts", source)
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/expression.ts")))
	if sourceFile == nil {
		t.Fatal("source file not found")
	}
	project, err := OpenProject(ProjectOptions{
		ProjectDir: directory,
		RootDir:    "src",
		OutDir:     "out",
		Config:     config.FlameworkConfig{},
	})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	state, err := newTransformState(TransformInput{
		Program: program,
		Checker: checker,
		Files:   []*ast.SourceFile{sourceFile},
		Project: project,
	}, nil)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	return state, sourceFile
}

func findExpressionKind(t *testing.T, root *ast.Node, kind ast.Kind) *ast.Node {
	t.Helper()
	var found *ast.Node
	var visit func(*ast.Node) bool
	visit = func(node *ast.Node) bool {
		if node.Kind == kind {
			found = node
			return true
		}
		return node.ForEachChild(visit)
	}
	visit(root)
	if found == nil {
		t.Fatalf("expression kind %v not found", kind)
	}
	return found
}

func findNamedClass(t *testing.T, root *ast.Node, name string) *ast.Node {
	t.Helper()
	var found *ast.Node
	var visit func(*ast.Node) bool
	visit = func(node *ast.Node) bool {
		if ast.IsClassDeclaration(node) && node.Name() != nil && node.Name().Text() == name {
			found = node
			return true
		}
		return node.ForEachChild(visit)
	}
	visit(root)
	if found == nil {
		t.Fatalf("class %q not found", name)
	}
	return found
}
