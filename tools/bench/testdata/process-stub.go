// Prebuild this test helper; the harness never compiles it inside an invocation.
package main

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"time"
)

func main() {
	if len(os.Args) > 1 && os.Args[1] == "--hold-stream" {
		time.Sleep(250 * time.Millisecond)
		fmt.Fprintln(os.Stderr, "descendant stream tail")
		return
	}
	if len(os.Args) > 1 && os.Args[1] == "--hang" {
		_ = os.WriteFile(os.Getenv("ROTOR_BENCH_TEST_PID"), []byte(fmt.Sprint(os.Getpid())), 0o600)
		time.Sleep(time.Minute)
		return
	}
	project, timings := "", ""
	for i := 1; i+1 < len(os.Args); i++ {
		switch os.Args[i] {
		case "--project":
			project = os.Args[i+1]
		case "--timings":
			timings = os.Args[i+1]
		}
	}
	fmt.Fprintln(os.Stderr, "stub stderr before completion")
	switch os.Getenv("ROTOR_BENCH_TEST_MODE") {
	case "hang", "stream-tail":
		arg := "--hold-stream"
		if os.Getenv("ROTOR_BENCH_TEST_MODE") == "hang" {
			arg = "--hang"
		}
		child := exec.Command(os.Args[0], arg)
		child.Stdout, child.Stderr = os.Stdout, os.Stderr
		if err := child.Start(); err != nil {
			panic(err)
		}
		// Keep the parent alive long enough to attach it to the harness job.
		time.Sleep(50 * time.Millisecond)
	case "compiler-persistence":
		time.Sleep(180 * time.Millisecond)
	case "mutate-binary":
		target, _ := os.Stat(os.Getenv("ROTOR_BENCH_TEST_MUTATE"))
		self, _ := os.Stat(os.Args[0])
		if target != nil && self != nil && os.SameFile(target, self) {
			break
		}
		f, err := os.OpenFile(os.Getenv("ROTOR_BENCH_TEST_MUTATE"), os.O_APPEND|os.O_WRONLY, 0)
		if err != nil {
			panic(err)
		}
		_, _ = f.WriteString("identity changed")
		_ = f.Close()
	case "mutate-fixture":
		_ = os.WriteFile(os.Getenv("ROTOR_BENCH_TEST_MUTATE"), []byte("changed"), 0o600)
	case "invalid":
		fmt.Print("incomplete JSON {")
		return
	}
	if project != "" {
		_ = os.MkdirAll(filepath.Join(project, "out"), 0o755)
		_ = os.WriteFile(filepath.Join(project, "out", "result.luau"), []byte("return 1\n"), 0o600)
	}
	if timings != "" {
		_ = os.WriteFile(timings, []byte(`{"schema":2,"totalMs":1}`), 0o600)
	}
	if os.Getenv("ROTOR_BENCH_TEST_MODE") == "not-ok" {
		fmt.Println(`{"ok":false,"diagnostics":[]}`)
		return
	}
	fmt.Println(`{"ok":true,"diagnostics":[]}`)
	if os.Getenv("ROTOR_BENCH_TEST_MODE") == "nonzero" {
		os.Exit(9)
	}
}
