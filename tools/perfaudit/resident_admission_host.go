package main

import (
	"crypto/sha256"
	"fmt"
	"math"
	"strings"
)

func (r *residentReader) hostIdentity(host object, p string) {
	for _, k := range []string{"os", "architecture", "powershell", "power_scheme"} {
		if r.str(host, k, p) == "" {
			r.invalid(p+"."+k, "nonempty host identity required")
		}
	}
	if r.uint(host, "processors", p, math.MaxInt32) == 0 {
		r.invalid(p, "positive processor count required")
	}
	env := r.obj(host["environment"], p+".environment", "", "")
	seen := map[string]bool{}
	for name := range env {
		key := strings.ToUpper(name)
		if seen[key] {
			r.invalid(p, "case-aliased environment variable")
		}
		seen[key] = true
		if !isSHA256(r.str(env, name, p)) {
			r.invalid(p, "environment values must be SHA256 hashes")
		}
	}
}

func (r *residentReader) runtimeEnvironment(report, host object, p string) {
	runtime := r.obj(report["runtime"], p+".runtime", "goVersion goos goarch gomaxprocs numCPU inheritedEnv", "")
	inherited := r.obj(runtime["inheritedEnv"], p+".inheritedEnv", "GOGC GOMAXPROCS GOMEMLIMIT GODEBUG", "")
	env := r.obj(host["environment"], p+".environment", "", "")
	for _, name := range []string{"GOGC", "GOMAXPROCS", "GOMEMLIMIT", "GODEBUG"} {
		value := r.str(inherited, name, p)
		found := false
		for key := range env {
			if strings.EqualFold(key, name) {
				found = true
				if r.str(env, key, p) != fmt.Sprintf("%x", sha256.Sum256([]byte(value))) {
					r.invalid(p+"."+name, "runtime value differs from retained host environment hash")
				}
			}
		}
		if !found && value != "" {
			r.invalid(p+"."+name, "nonempty runtime override absent from host environment")
		}
	}
	r.equal(runtime["numCPU"], host["processors"], p+".host.processors")
}
