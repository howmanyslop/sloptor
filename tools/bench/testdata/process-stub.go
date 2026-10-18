// Prebuild this test helper; the harness never compiles it inside an invocation.
package main

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"time"
)

func main() {
	if len(os.Args) > 1 && (os.Args[1] == "--persist" || os.Args[1] == "--persist-ready") {
		if err := os.WriteFile(os.Getenv("ROTOR_BENCH_TEST_PID"), []byte(fmt.Sprint(os.Getpid())), 0o600); err != nil {
			panic(err)
		}
		if os.Args[1] == "--persist-ready" {
			// The root retains the control writer until exit, so this child
			// cannot finish between its readiness signal and deliberate exit 9.
			fmt.Fprintln(os.Stdout, os.Getpid())
			released := make(chan struct{})
			go func() { _, _ = io.Copy(io.Discard, os.Stdin); close(released) }()
			select {
			case <-released:
			case <-time.After(3 * time.Second):
				panic("negative-child control pipe did not close")
			}
		}
		time.Sleep(350 * time.Millisecond)
		_ = os.WriteFile(os.Getenv("ROTOR_BENCH_TEST_DONE"), []byte("persistence complete"), 0o600)
		return
	}
	if os.Getenv("ROTOR_BENCH_TEST_MODE") == "echo" {
		cwd, _ := os.Getwd()
		_ = json.NewEncoder(os.Stdout).Encode(map[string]any{"arguments": os.Args[1:], "cwd": cwd, "value": os.Getenv("ROTOR_BENCH_TEST_VALUE")})
		return
	}
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
	mode := os.Getenv("ROTOR_BENCH_TEST_MODE")
	if mode == "abrupt" {
		child := exec.Command(os.Args[0], "--hang")
		if err := child.Start(); err != nil {
			panic(err)
		}
		fmt.Println(`{"ok":true,"diagnostics":[]}`)
		fmt.Fprintln(os.Stderr, "ABRUPT_EVIDENCE_ALREADY_EMITTED")
		time.Sleep(time.Minute)
		return
	}
	if mode == "raw-json" {
		fmt.Print(os.Getenv("ROTOR_BENCH_TEST_JSON"))
		return
	}
	if mode == "overflow" {
		fmt.Print(string(make([]byte, 32768)))
		return
	}
	if mode == "closed-success" || mode == "closed-private" || mode == "closed-nonzero" || mode == "closed-invalid" || mode == "closed-not-ok" || mode == "closed-timeout" {
		child := exec.Command(os.Args[0], "--persist")
		if mode == "closed-timeout" {
			// Root succeeds; a private child must still hit the tree deadline.
			child = exec.Command(os.Args[0], "--hang")
		}
		if mode == "closed-nonzero" {
			child = exec.Command(os.Args[0], "--persist-ready")
			control, err := child.StdinPipe()
			if err != nil {
				panic(err)
			}
			readyPipe, err := child.StdoutPipe()
			if err != nil {
				panic(err)
			}
			if err := child.Start(); err != nil {
				panic(err)
			}
			ready := make(chan bool, 1)
			go func() {
				var childID int
				_, err := fmt.Fscan(readyPipe, &childID)
				ready <- err == nil && childID == child.Process.Pid
			}()
			select {
			case valid := <-ready:
				if !valid {
					panic("negative-child readiness failed")
				}
			case <-time.After(2 * time.Second):
				panic("negative-child readiness timed out")
			}
			fmt.Fprintln(os.Stderr, "negative child ready:", child.Process.Pid)
			fmt.Println(`{"ok":true,"diagnostics":[]}`)
			runtime.KeepAlive(control)
			os.Exit(9)
		}
		if mode == "closed-private" {
			// Match Rotor's sidecar shape: separate stdin/stdout/stderr pipes.
			_, _ = child.StdinPipe()
			_, _ = child.StdoutPipe()
			_, _ = child.StderrPipe()
		}
		if err := child.Start(); err != nil {
			panic(err)
		}
		if mode == "closed-invalid" {
			fmt.Print("invalid {")
			return
		}
	}
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
		// Let the inherited-stream helper initialize before timeout probes.
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
		_ = os.WriteFile(timings, []byte(`{"schemaVersion":2,"ok":true,"totalMs":1,"stageSemantics":"workMs","stages":{},"counts":{},"metadata":{}}`), 0o600)
	}
	if mode == "not-ok" || mode == "closed-not-ok" {
		fmt.Println(`{"ok":false,"diagnostics":[]}`)
		return
	}
	fmt.Println(`{"ok":true,"diagnostics":[]}`)
	if mode == "nonzero" || mode == "closed-nonzero" {
		os.Exit(9)
	}
}
