// Prebuild this test helper; the harness never compiles it inside an invocation.
package main

import (
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"time"
)

func main() {
	if len(os.Args) > 1 && os.Args[1] == "--resource-worker" {
		if len(os.Args) > 5 {
			parentID, err := strconv.Atoi(os.Args[5])
			if err != nil {
				panic(err)
			}
			// Windows permits waiting on this retained parent process object.
			// The harness owns its original handle, preventing PID reuse here.
			parent, err := os.FindProcess(parentID)
			if err != nil {
				panic(err)
			}
			if _, err := parent.Wait(); err != nil {
				panic(err)
			}
			if err := os.WriteFile(filepath.Join(os.Args[3], os.Args[4]+"-parent-exited"), []byte("parent exit observed"), 0o600); err != nil {
				panic(err)
			}
		}
		resourceWorker(os.Args[2], os.Args[3], os.Args[4])
		return
	}
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
	if mode == "stable-state" {
		stableState(project)
	}
	if len(mode) > len("resource-") && mode[:len("resource-")] == "resource-" {
		resourceControl(mode[len("resource-"):], os.Getenv("ROTOR_BENCH_TEST_RESOURCE_ROOT"))
		fmt.Println(`{"ok":true,"diagnostics":[]}`)
		return
	}
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
	if project != "" && mode != "stable-state" {
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

// Include literal cwd/source-directory time, as real copy manifests/maps do.
// Cache identity deliberately differs between arms; emitted bytes do not.
func stableState(project string) {
	writeChanged := func(path string, bytes []byte) {
		old, err := os.ReadFile(path)
		if err == nil && string(old) == string(bytes) {
			return
		}
		if err := os.WriteFile(path, bytes, 0o600); err != nil {
			panic(err)
		}
	}
	cache := filepath.Join(project, ".rotor")
	if err := os.MkdirAll(cache, 0o755); err != nil {
		panic(err)
	}
	arm := filepath.Base(os.Args[0])
	identity := filepath.Join(cache, "arm")
	if old, err := os.ReadFile(identity); err == nil && string(old) != arm {
		panic("used the other arm's warm state")
	}
	writeChanged(identity, []byte(arm))
	countPath := filepath.Join(cache, "count")
	previous, _ := os.ReadFile(countPath)
	count, _ := strconv.Atoi(string(previous))
	writeChanged(countPath, []byte(strconv.Itoa(count+1)))
	source, err := os.Stat(filepath.Join(project, "src"))
	if err != nil {
		panic(err)
	}
	output := filepath.Join(project, "out")
	if err := os.MkdirAll(output, 0o755); err != nil {
		panic(err)
	}
	bytes, err := json.Marshal(map[string]any{"project": project, "source": filepath.Join(project, "src", "input.ts"), "source_directory_mtime": source.ModTime().UTC().Format(time.RFC3339Nano)})
	if err != nil {
		panic(err)
	}
	writeChanged(filepath.Join(output, "rbxts.copyfiles.json"), bytes)
	writeChanged(filepath.Join(output, "result.luau.map"), bytes)
	writeChanged(filepath.Join(output, "result.luau"), []byte("return 1\n"))
}

func resourceWorker(kind, directory, tag string) {
	write := func(name string, bytes []byte) {
		if err := os.WriteFile(filepath.Join(directory, tag+"-"+name), bytes, 0o600); err != nil {
			panic(err)
		}
	}
	write("pid", []byte(strconv.Itoa(os.Getpid())))
	switch kind {
	case "cpu":
		var sum [32]byte
		for range 1_000_000 {
			sum = sha256.Sum256(sum[:])
		}
		write("cpu", []byte(fmt.Sprintf("%x", sum)))
	case "io":
		bytes := make([]byte, 16*1024*1024)
		for index := range bytes {
			bytes[index] = 0x5a
		}
		write("io.bin", bytes)
		read, err := os.ReadFile(filepath.Join(directory, tag+"-io.bin"))
		if err != nil || sha256.Sum256(read) != sha256.Sum256(bytes) {
			panic("resource I/O verification failed")
		}
		write("io-verified", []byte(fmt.Sprintf("%x", sha256.Sum256(read))))
	case "memory", "memory-gated":
		bytes := make([]byte, 128*1024*1024)
		for index := 0; index < len(bytes); index += 4096 {
			bytes[index] = 0x5a
		}
		write("ready", []byte("allocated and touched"))
		if kind == "memory-gated" {
			deadline := time.Now().Add(5 * time.Second)
			for {
				if _, err := os.Stat(filepath.Join(directory, "gate")); err == nil {
					break
				}
				if time.Now().After(deadline) {
					panic("resource allocation gate timed out")
				}
				time.Sleep(5 * time.Millisecond)
			}
		}
		time.Sleep(100 * time.Millisecond)
		if bytes[len(bytes)-4096] != 0x5a {
			panic("resource allocation lost")
		}
		runtime.KeepAlive(bytes)
	default:
		panic("unknown resource worker")
	}
	write("done", []byte("complete"))
}

func resourceControl(kind, directory string) {
	if err := os.MkdirAll(directory, 0o755); err != nil {
		panic(err)
	}
	spawn := func(worker, tag string) *exec.Cmd {
		args := []string{"--resource-worker", worker, directory, tag}
		if kind == "child-cpu" || kind == "child-io" {
			args = append(args, strconv.Itoa(os.Getpid()))
		}
		child := exec.Command(os.Args[0], args...)
		// Private streams; Go does not kill this child when its root exits.
		if err := child.Start(); err != nil {
			panic(err)
		}
		return child
	}
	switch kind {
	case "leaf":
	case "cpu", "io":
		resourceWorker(kind, directory, "root")
	case "child-cpu", "child-io":
		spawn(kind[len("child-"):], "child")
	case "staggered":
		for _, tag := range []string{"a", "b"} {
			if err := spawn("memory", tag).Wait(); err != nil {
				panic(err)
			}
		}
	case "overlap":
		spawn("memory-gated", "a")
		spawn("memory-gated", "b")
		deadline := time.Now().Add(5 * time.Second)
		for {
			_, a := os.Stat(filepath.Join(directory, "a-ready"))
			_, b := os.Stat(filepath.Join(directory, "b-ready"))
			if a == nil && b == nil {
				break
			}
			if time.Now().After(deadline) {
				panic("overlapping resource allocations did not become ready")
			}
			time.Sleep(5 * time.Millisecond)
		}
		if err := os.WriteFile(filepath.Join(directory, "gate"), []byte("both live and touched"), 0o600); err != nil {
			panic(err)
		}
	case "timeout":
		resourceWorker("io", directory, "root")
		time.Sleep(10 * time.Second)
	case "nonzero":
		resourceWorker("io", directory, "root")
		os.Exit(9)
	default:
		panic("unknown resource control")
	}
}
