package main

import (
	"io"
	"os"
	"path/filepath"
	"runtime/debug"
	"runtime/metrics"
	"testing"

	"rotor/internal/logservice"
)

func TestAutomaticMemoryLimitBoundaries(t *testing.T) {
	for _, tt := range []struct {
		name   string
		memory int64
		want   int64
	}{
		{"negative", -1, 6 << 30},
		{"floor boundary", minimumLimit, minimumLimit},
		{"host fraction", 4 << 30, 3 << 30},
		{"largest host", 1<<63 - 1, maximumLimit},
	} {
		t.Run(tt.name, func(t *testing.T) {
			if got := automaticMemoryLimit(tt.memory); got != tt.want {
				t.Fatalf("automaticMemoryLimit(%d) = %d, want %d", tt.memory, got, tt.want)
			}
		})
	}
}

// Runtime settings and environment are process-wide: these tests deliberately
// stay sequential, and restore both even when an assertion fails.
func runtimePolicyTestBaseline(t *testing.T) {
	t.Helper()
	for _, name := range []string{"GOGC", "GOMEMLIMIT"} {
		t.Setenv(name, "")
		if err := os.Unsetenv(name); err != nil {
			t.Fatal(err)
		}
	}
	gc := debug.SetGCPercent(137)
	limit := debug.SetMemoryLimit(2 << 30)
	t.Cleanup(func() {
		debug.SetGCPercent(gc)
		debug.SetMemoryLimit(limit)
	})
}

func assertRuntimePolicy(t *testing.T, gc int, limit int64) {
	t.Helper()
	samples := []metrics.Sample{{Name: "/gc/gogc:percent"}}
	metrics.Read(samples)
	if got := samples[0].Value.Uint64(); got != uint64(gc) {
		t.Errorf("GOGC = %d, want %d", got, gc)
	}
	if got := debug.SetMemoryLimit(-1); got != limit {
		t.Errorf("memory limit = %d, want %d", got, limit)
	}
}

func TestRuntimePolicyScopes(t *testing.T) {
	runtimePolicyTestBaseline(t)
	oneShotLimit := automaticMemoryLimit(effectiveMemory())
	func() {
		defer applyRuntimePolicy(false)()
		assertRuntimePolicy(t, 400, oneShotLimit)
		func() {
			defer applyRuntimePolicy(true)()
			assertRuntimePolicy(t, 100, min(oneShotLimit, watchLimit))
		}()
		assertRuntimePolicy(t, 400, oneShotLimit)
	}()
	assertRuntimePolicy(t, 137, 2<<30)
}

func TestRuntimePolicyEnvironmentPrecedence(t *testing.T) {
	for _, mode := range []struct {
		name  string
		watch bool
	}{{"one-shot", false}, {"watch", true}} {
		for _, tt := range []struct {
			name, env, value string
		}{
			{"gc numeric", "GOGC", "75"},
			{"gc zero", "GOGC", "0"},
			{"gc disabled", "GOGC", "off"},
			{"gc empty", "GOGC", ""},
			{"limit numeric", "GOMEMLIMIT", "3GiB"},
			{"limit above watch cap", "GOMEMLIMIT", "32GiB"},
			{"limit disabled", "GOMEMLIMIT", "off"},
			{"limit empty", "GOMEMLIMIT", ""},
		} {
			t.Run(mode.name+"/"+tt.name, func(t *testing.T) {
				runtimePolicyTestBaseline(t)
				t.Setenv(tt.env, tt.value)
				baselineGC := 137
				baselineLimit := int64(2 << 30)
				if tt.env == "GOGC" && tt.value == "off" {
					debug.SetGCPercent(-1)
					baselineGC = -1
				}
				if tt.env == "GOMEMLIMIT" {
					switch tt.value {
					case "32GiB":
						baselineLimit = 32 << 30
					case "off":
						baselineLimit = 1<<63 - 1
					}
					debug.SetMemoryLimit(baselineLimit)
				}
				gc, limit := 400, automaticMemoryLimit(effectiveMemory())
				if mode.watch {
					gc, limit = 100, min(limit, watchLimit)
				}
				if tt.env == "GOGC" {
					gc = baselineGC
				} else {
					limit = baselineLimit
				}
				func() {
					defer applyRuntimePolicy(mode.watch)()
					assertRuntimePolicy(t, gc, limit)
				}()
				assertRuntimePolicy(t, baselineGC, baselineLimit)
			})
		}
	}
}

func TestRuntimePolicyRunRestoresDefaults(t *testing.T) {
	runtimePolicyTestBaseline(t)
	_, code := captureStdout(t, func() int { return run([]string{"--version"}) })
	if code != 0 {
		t.Fatalf("run exit = %d, want 0", code)
	}
	assertRuntimePolicy(t, 137, 2<<30)
}

type runtimePolicyObserved struct{}

type runtimePolicyWriter struct {
	t     *testing.T
	gc    int
	limit int64
}

func (w runtimePolicyWriter) Write([]byte) (int, error) {
	assertRuntimePolicy(w.t, w.gc, w.limit)
	// Stop at the banner or initial buildStart event, before compilation.
	// Unwinding also verifies the command restores its scoped runtime settings.
	panic(runtimePolicyObserved{})
}

func TestRuntimePolicyWatchInvocations(t *testing.T) {
	for _, tt := range []struct {
		name  string
		args  []string
		watch bool
	}{
		{"build", []string{"build", "-w"}, true},
		{"build json", []string{"build", "--watch", "--json"}, true},
		{"solution", []string{"build", "--build", "-w"}, true},
		{"solution json", []string{"build", "--build", "-w", "--json"}, true},
		{"compiler alias", []string{"--build", "-w"}, true},
		{"build watch false", []string{"build", "--watch=false"}, false},
		{"build no watch", []string{"build", "--no-watch"}, false},
		{"check", []string{"check", "-w"}, true},
		{"check json", []string{"check", "-w", "--json"}, true},
		{"check watch false", []string{"check", "--watch=false"}, false},
	} {
		t.Run(tt.name, func(t *testing.T) {
			runtimePolicyTestBaseline(t)
			t.Setenv("NO_COLOR", "1")
			dir := t.TempDir()
			config := `{"files":[]}`
			if tt.name == "solution json" {
				// A graph error reports buildStart on the command goroutine,
				// allowing the probe to stop before the async solution drain.
				config = `{"files":[],"references":[{"path":"missing"}]}`
			}
			if err := os.WriteFile(filepath.Join(dir, "tsconfig.json"), []byte(config), 0o644); err != nil {
				t.Fatal(err)
			}
			// Watch JSON commands redirect the global log channel.
			previousLog := logservice.Output
			t.Cleanup(func() { logservice.Output = previousLog })
			gc, limit := 137, int64(2<<30)
			if tt.watch {
				gc, limit = 100, min(automaticMemoryLimit(effectiveMemory()), watchLimit)
			}
			func() {
				defer func() {
					if p := recover(); p != nil {
						if _, ok := p.(runtimePolicyObserved); !ok {
							panic(p)
						}
					} else {
						t.Error("command returned before reaching the initial banner/buildStart")
					}
				}()
				args := append(append([]string{}, tt.args...), dir)
				execute(args, cliStreams{out: runtimePolicyWriter{t, gc, limit}, err: io.Discard})
			}()
			assertRuntimePolicy(t, 137, 2<<30)
		})
	}
}
