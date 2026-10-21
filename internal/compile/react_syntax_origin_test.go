package compile

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestReactSyntaxOriginSurvivesBothTransports(t *testing.T) {
	dir := writeProject(t, "@scope/react-origin", "")
	projectDir, program, diags, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (%v)", err, diags)
	}
	files := projectSourceFiles(program)
	call := transformerCall{dir: projectDir, configPath: program.Options().ConfigFilePath, compileFiles: files, stampFiles: files}
	workerDir := t.TempDir()
	// Observe the actual stdio request after argument expansion/reconstruction.
	// No TypeScript or transformer mock is used by this transport-only fixture.
	script := `const fs = require("node:fs");
const path = require("node:path");
require("node:readline").createInterface({input:process.stdin}).on("line", line => {
  fs.writeFileSync(path.join(__dirname, "request.json"), line);
  process.stdout.write(JSON.stringify({diagnostics:[],transformed:[]}) + "\n");
});`
	if err := os.WriteFile(filepath.Join(workerDir, "main.js"), []byte(script), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Setenv("ROTOR_SIDECAR_PATH", workerDir)
	closeSidecarSessions()
	t.Cleanup(closeSidecarSessions)
	var callbackRequest TransformerRequest
	callback := newCallbackTransformerHost(func(_ context.Context, request TransformerRequest) (TransformerResponse, error) {
		callbackRequest = request
		return TransformerResponse{}, nil
	})
	for _, transport := range []string{"callback", "stdio"} {
		host := callback
		if transport == "stdio" {
			host = sidecarTransformerHost{}
		}
		readRequest := func() []byte {
			t.Helper()
			var data []byte
			var err error
			if transport == "callback" {
				data, err = json.Marshal(callbackRequest)
			} else {
				data, err = os.ReadFile(filepath.Join(workerDir, "request.json"))
			}
			if err != nil {
				t.Fatal(err)
			}
			return data
		}
		for _, test := range []struct{ enabled, origin, want string }{
			{"0", transformerOriginalAuthored, ""},
			{"1", "", ""},
			{"1", "unknown", ""},
			{"1", transformerOriginalAuthored, transformerOriginalAuthored},
		} {
			t.Setenv("ROTOR_REACT_SYNTAX_HOST", test.enabled)
			call.sourceOrigin = test.origin
			if _, _, err := host.transform(context.Background(), call); err != nil {
				t.Fatalf("%s transform: %v", transport, err)
			}
			var wire map[string]json.RawMessage
			if err := json.Unmarshal(readRequest(), &wire); err != nil {
				t.Fatal(err)
			}
			value, present := wire["sourceOrigin"]
			if present != (test.want != "") || (present && string(value) != `"`+test.want+`"`) {
				t.Fatalf("%s enabled=%s origin=%q: sourceOrigin=%s present=%v", transport, test.enabled, test.origin, value, present)
			}
		}
		if _, _, err := host.validate(context.Background(), call.dir, call.configPath); err != nil {
			t.Fatal(err)
		}
		if strings.Contains(string(readRequest()), `"sourceOrigin"`) {
			t.Fatalf("%s validation carried authored provenance", transport)
		}
	}
}

func TestReactSyntaxOriginInitialPrefixSuffixAndNextBuild(t *testing.T) {
	t.Setenv("ROTOR_REACT_SYNTAX_HOST", "1")
	dir := writeProject(t, "@scope/react-stages", "")
	configPath := filepath.Join(dir, "tsconfig.json")
	config, err := os.ReadFile(configPath)
	if err != nil {
		t.Fatal(err)
	}
	config = []byte(strings.Replace(string(config), `"types": []`, `"types": [], "plugins": [{"transform":"prefix"},{"transform":"suffix"}]`, 1))
	if err := os.WriteFile(configPath, config, 0o644); err != nil {
		t.Fatal(err)
	}
	mainPath := filepath.Join(dir, "src", "main.ts")
	const authored = "export const value=1;\n"
	if err := os.WriteFile(mainPath, []byte(authored), 0o644); err != nil {
		t.Fatal(err)
	}
	projectDir, program, diags, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (%v)", err, diags)
	}
	var requests []TransformerRequest
	ctx := WithTransformerCallback(context.Background(), func(_ context.Context, request TransformerRequest) (TransformerResponse, error) {
		requests = append(requests, request)
		return TransformerResponse{}, nil
	})
	files := projectSourceFiles(program)
	if _, diags, err := prepareTransformerProgramWithHost(ctx, transformerHostFromContext(ctx), projectDir, program, files, nil); err != nil {
		t.Fatalf("initial stage: %v (%v)", err, diags)
	}
	if len(requests) != 1 || requests[0].SourceOrigin != transformerOriginalAuthored {
		t.Fatalf("initial request provenance: %#v", requests)
	}
	requests = nil
	if err := os.WriteFile(filepath.Join(dir, "rotor.toml"), []byte("[flamework]\nafter = \"prefix\"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	// Native Flamework reprints the source between prefix and suffix. The next
	// build must send disk text to revert that session overlay before admission.
	for build := 0; build < 2; build++ {
		if _, diags, err := prepareCompilePipeline(projectDir, program, files, nil, ProjectOptions{Context: ctx}); err != nil {
			t.Fatalf("pipeline build %d: %v (%v)", build, err, diags)
		}
	}
	if len(requests) != 4 {
		t.Fatalf("stage requests = %d, want 4", len(requests))
	}
	for index, request := range requests {
		want := ""
		if index%2 == 0 {
			want = transformerOriginalAuthored
		}
		if request.SourceOrigin != want {
			t.Fatalf("stage %d origin=%q, want %q", index, request.SourceOrigin, want)
		}
	}
	textFor := func(request TransformerRequest) string {
		for _, file := range request.ChangedFiles {
			if filepath.Clean(file.FileName) == filepath.Clean(mainPath) {
				return file.Text
			}
		}
		return ""
	}
	if generated := textFor(requests[1]); generated == "" || generated == authored {
		t.Fatalf("suffix did not receive native-generated text: %q", generated)
	}
	if got := textFor(requests[2]); got != authored {
		t.Fatalf("next authored prefix retained suffix overlay: %q", got)
	}
}
