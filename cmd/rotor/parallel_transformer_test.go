package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"testing"

	"rotor/internal/logservice"
)

func TestCmdBuildSolutionReachesParallelTransformer(t *testing.T) {
	// Exercise the actual command parser/body, --build coordinator and normal
	// worker transport. No fabricated oneShotBuild flag or builder replacement.
	dir := writeBuildableProject(t, "")
	_, file, _, _ := runtime.Caller(0)
	worker := filepath.Clean(filepath.Join(filepath.Dir(file), "../../tools/sidecar"))
	t.Setenv("ROTOR_SIDECAR_PATH", worker)
	pluginDir := filepath.Join(dir, "plugins", "reviewed")
	mustWrite(t, filepath.Join(pluginDir, "package.json"), `{"name":"reviewed-cli-fixture","main":"index.js"}`)
	mustWrite(t, filepath.Join(pluginDir, "index.js"), `require('node:fs').writeFileSync('worker-'+process.pid+'.pid',String(process.pid));
module.exports = () => () => source => source;`)
	t.Cleanup(func() {
		entries, _ := os.ReadDir(dir)
		for _, entry := range entries {
			if strings.HasPrefix(entry.Name(), "worker-") && strings.HasSuffix(entry.Name(), ".pid") {
				text, _ := os.ReadFile(filepath.Join(dir, entry.Name()))
				if pid, err := strconv.Atoi(string(text)); err == nil {
					if process, err := os.FindProcess(pid); err == nil {
						_ = process.Kill()
						_ = process.Release()
					}
				}
			}
		}
	})
	mustWrite(t, filepath.Join(dir, "src", "second.ts"), "export {};\n")
	configBytes, err := os.ReadFile(filepath.Join(dir, "tsconfig.json"))
	if err != nil {
		t.Fatal(err)
	}
	var project map[string]any
	if err := json.Unmarshal(configBytes, &project); err != nil {
		t.Fatal(err)
	}
	options := project["compilerOptions"].(map[string]any)
	options["plugins"] = []any{map[string]any{"transform": "./plugins/reviewed"}}
	encoded, err := json.Marshal(project)
	if err != nil {
		t.Fatal(err)
	}
	configPath := filepath.Join(dir, "tsconfig.lib.json")
	mustWrite(t, configPath, string(encoded))
	identity := exec.Command("node", "-e", `const path=require('node:path'); const [worker,project,entry]=process.argv.slice(1);
process.stdout.write(JSON.stringify({version:require(path.join(worker,'index.js')).resolveTypeScript(project).version,digest:require(path.join(worker,'lib/parallel.js')).packageDigest(entry)}));`, worker, dir, filepath.Join(pluginDir, "index.js"))
	output, err := identity.Output()
	if err != nil {
		t.Fatalf("synthetic fixture identity: %v", err)
	}
	var profile struct{ Version, Digest string }
	if err := json.Unmarshal(output, &profile); err != nil {
		t.Fatal(err)
	}
	mustWrite(t, filepath.Join(dir, "rotor.toml"), fmt.Sprintf(`[flamework]
after = "./plugins/reviewed"
[parallelTransformers]
workers = 2
batchIndependent = true
typescript = %q
[[parallelTransformers.plugins]]
config = '{"transform":"./plugins/reviewed"}'
sha256 = %q
`, profile.Version, profile.Digest))
	var stages bytes.Buffer
	previousOutput, previousVerbose := logservice.Output, logservice.Verbose
	logservice.Output = &stages
	defer func() { logservice.Output, logservice.Verbose = previousOutput, previousVerbose }()
	stdout, stderr, code := captureBuildOutput(t, []string{"--build", "--project", configPath, "--verbose", "--noInclude"})
	if code != 0 || !strings.Contains(stages.String(), "transformer parallel batch completed (workers=2)") {
		t.Fatalf("actual solution CLI did not complete a split: exit=%d\n%s\n%s\n%s", code, stdout, stderr, stages.String())
	}
}
