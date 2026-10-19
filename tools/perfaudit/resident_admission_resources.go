package main

import (
	"bytes"
	"encoding/json"
	"math"
	"slices"
	"strings"
)

// Resident raw native components use unsigned ticks and arbitrary-size sums in
// the reviewed core. No fresh-process wall/population constructor is called.
func (r *residentReader) resources(raw json.RawMessage, p string, layout *uint64) ResidentResources {
	result := ResidentResources{}
	o := r.obj(raw, p, "Schema Status Completion CompletionError RootCpu OtherOwnedCpu QueryElapsedMS", "JobAccounting JobMemory")
	r.number(o, "Schema", p, 1)
	r.want(o, "Completion", p, "complete")
	resourceNull(&r.validation, o, "CompletionError", p)
	resourceNonnegative(&r.validation, o, "QueryElapsedMS", p, true)
	components := map[string]object{}
	statuses := []string{}
	ioNames := []string{"ReadOperations", "WriteOperations", "OtherOperations", "ReadBytes", "WriteBytes", "OtherBytes"}
	for _, name := range []string{"JobAccounting", "JobMemory", "RootCpu", "OtherOwnedCpu"} {
		q := p + "." + name
		if _, exists := o[name]; !exists && (name == "JobAccounting" || name == "JobMemory") {
			r.add(residentIncomplete, q, "required native component absent; no zero substitution")
			statuses = append(statuses, "unavailable")
			continue
		}
		c := r.obj(o[name], q, "Status Error Values", "")
		status := r.str(c, "Status", q)
		statuses = append(statuses, status)
		if !slices.Contains([]string{"available", "failed", "unsupported", "unavailable"}, status) {
			r.invalid(q, "unknown native component status")
		}
		if status != "available" {
			resourceNull(&r.validation, c, "Values", q)
			if r.str(c, "Error", q) == "" {
				r.invalid(q, "missing unavailable-component cause")
			}
			if name == "JobAccounting" || name == "JobMemory" {
				r.add(residentIncomplete, q, "required native component unavailable; no zero substitution")
			}
			continue
		}
		resourceNull(&r.validation, c, "Error", q)
		keys := "UserTicks100ns KernelTicks100ns"
		switch name {
		case "JobAccounting":
			keys += " TotalProcesses ActiveProcesses LimitTerminatedProcesses TotalPageFaults ReadOperations WriteOperations OtherOperations ReadBytes WriteBytes OtherBytes StructureBytes"
		case "JobMemory":
			keys = "PeakJobMemoryBytes PeakProcessMemoryBytes StructureBytes"
		}
		required, measurements := keys, ""
		switch name {
		case "JobAccounting":
			required = "TotalProcesses ActiveProcesses LimitTerminatedProcesses TotalPageFaults StructureBytes"
			measurements = "UserTicks100ns KernelTicks100ns ReadOperations WriteOperations OtherOperations ReadBytes WriteBytes OtherBytes"
		case "JobMemory":
			required, measurements = "StructureBytes", "PeakJobMemoryBytes PeakProcessMemoryBytes"
		}
		values := r.obj(c["Values"], q+".Values", required, measurements)
		components[name] = values
		var size uint64
		if name == "JobAccounting" || name == "JobMemory" {
			size = r.uint(values, "StructureBytes", q, math.MaxInt32)
			if !resourceNativeStructureSize(name, size) {
				r.invalid(q, "unsupported fixed native structure size")
			}
			if name == "JobMemory" {
				if *layout == 0 {
					*layout = size
				} else if *layout != size {
					r.invalid(q, "collector memory layout changed within batch")
				}
			}
		}
		for _, k := range strings.Fields(keys) {
			if _, exists := values[k]; !exists && slices.Contains(strings.Fields(measurements), k) {
				r.add(residentIncomplete, q+"."+k, "required native integer absent; no zero substitution")
				continue
			}
			max := uint64(math.MaxUint64)
			if slices.Contains([]string{"TotalProcesses", "ActiveProcesses", "LimitTerminatedProcesses", "TotalPageFaults"}, k) {
				max = math.MaxUint32
			}
			if name == "JobMemory" && size == 112 && k != "StructureBytes" {
				max = math.MaxUint32
			}
			r.uint(values, k, q, max)
		}
		get := func(k string) *uint64 {
			if _, exists := values[k]; !exists {
				return nil
			}
			v := r.uint(values, k, q, math.MaxUint64)
			return &v
		}
		if name == "JobAccounting" {
			result.UserTicks, result.KernelTicks = get("UserTicks100ns"), get("KernelTicks100ns")
			for i, k := range ioNames {
				result.IO[i] = get(k)
			}
			r.number(values, "ActiveProcesses", q, 0)
			if r.uint(values, "TotalProcesses", q, math.MaxUint32) == 0 {
				r.invalid(q, "owned job must include a process")
			}
			r.number(values, "LimitTerminatedProcesses", q, 0)
		}
		if name == "JobMemory" {
			result.PeakBytes = get("PeakJobMemoryBytes")
		}
	}
	all := func(w string) bool {
		for _, s := range statuses {
			if s != w {
				return false
			}
		}
		return true
	}
	expected := "failed"
	switch {
	case all("available"):
		expected = "available"
	case slices.Contains(statuses, "available"):
		expected = "partial"
	case all("unsupported"):
		expected = "unsupported"
	case all("unavailable"):
		expected = "unavailable"
	}
	r.want(o, "Status", p, expected)
	if slices.Contains(statuses, "available") && bytes.Equal(bytes.TrimSpace(o["QueryElapsedMS"]), []byte("null")) {
		r.invalid(p, "available getter needs query duration")
	}
	if other, ok := components["OtherOwnedCpu"]; ok {
		job, jok := components["JobAccounting"]
		root, rok := components["RootCpu"]
		if !jok || !rok {
			r.invalid(p, "derived CPU lacks job/root components")
		} else {
			for _, k := range []string{"UserTicks100ns", "KernelTicks100ns"} {
				if _, exists := job[k]; !exists {
					continue
				}
				j, a, b := r.uint(job, k, p, math.MaxUint64), r.uint(root, k, p, math.MaxUint64), r.uint(other, k, p, math.MaxUint64)
				if j < a || b != j-a {
					r.invalid(p, "derived CPU differs from exact job minus root")
				}
			}
		}
	}
	return result
}

func (r *residentReader) native(entry, plan object, p string, layout *uint64) ResidentResources {
	text := r.str(entry, "native_result_json", p)
	native := r.obj([]byte(text), p+".native_result_json", "DurationMS ElapsedMS ExitCode Status Error EvidencePersistenceMS CaptureLimitBytes ResourceAccounting StdoutBytes StderrBytes CaptureOverflow CaptureSpoolWriteMS", "")
	outer := r.obj(entry["result"], p+".result", "DurationMS ElapsedMS ExitCode Status Error EvidencePersistenceMS CaptureLimitBytes ResourceAccounting StdoutBytes StderrBytes CaptureOverflow CaptureSpoolWriteMS", "")
	// Both copies remain independently retained. A partial outer annotation must
	// never overwrite the original native serialization or become complete data.
	if r.str(entry, "status", p) == "completed" {
		r.equal(entry["result"], []byte(text), p+".native/outer")
	}
	r.want(native, "Status", p, "completed")
	r.number(native, "ExitCode", p, 0)
	resourceNull(&r.validation, native, "Error", p)
	for _, k := range []string{"ElapsedMS", "EvidencePersistenceMS", "CaptureSpoolWriteMS"} {
		resourceNonnegative(&r.validation, native, k, p, false)
	}
	duration := r.uint(native, "DurationMS", p, math.MaxInt64)
	if duration == 0 {
		r.invalid(p, "native positive rounded wall duration required")
	}
	var elapsed float64
	if json.Unmarshal(native["ElapsedMS"], &elapsed) == nil && (math.IsNaN(elapsed) || math.IsInf(elapsed, 0) || elapsed > float64(math.MaxInt64/2) || uint64(math.Max(1, math.RoundToEven(elapsed))) != duration) {
		r.invalid(p, "native wall fields contradict helper normalization; not a cycle estimand")
	}
	capture := r.uint(plan, "captureBytes", p, 1<<30)
	r.number(native, "CaptureLimitBytes", p, capture)
	r.flag(native, "CaptureOverflow", p, false)
	for _, stream := range []struct{ field, path string }{{"StdoutBytes", "stdout_path"}, {"StderrBytes", "stderr_path"}} {
		n := r.uint(native, stream.field, p, math.MaxInt64)
		if n > capture {
			r.invalid(p, "capture exceeded declared stream limit")
		}
		s := r.snapshot(r.str(entry, stream.path, p), false)
		if uint64(s.Bytes) != n {
			r.invalid(p, "retained stream length differs from native capture")
		}
	}
	// Validate outer component status even when it truthfully records partial
	// evidence. Its raw state remains separate from the native component values.
	r.str(outer, "Status", p)
	return r.resources(native["ResourceAccounting"], p+".native_resources", layout)
}
