package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"math"
	"os"
)

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}

func run(args []string, stdout, stderr io.Writer) int {
	flags := flag.NewFlagSet("perfaudit", flag.ContinueOnError)
	flags.SetOutput(stderr)
	input := flags.String("input", "", "schema1 manifest path (read only)")
	output := flags.String("output", "", "new audit JSON path; existing files are preserved")
	primary := flags.String("primary", "", "preregistered primary phase: cold or no_change (required)")
	limit := flags.Float64("max-primary-ratio", 0.95, "preregistered practical paired ratio threshold (must be explicit)")
	coldCount := flags.Int("cold-pairs", 30, "exact preregistered cold pair count; even, at least 10")
	noChangeCount := flags.Int("no-change-pairs", 100, "exact preregistered no_change pair count; even, at least 20")
	if err := flags.Parse(args); err != nil {
		if err == flag.ErrHelp {
			return 0
		}
		return 2
	}
	explicitLimit := false
	flags.Visit(func(f *flag.Flag) {
		if f.Name == "max-primary-ratio" {
			explicitLimit = true
		}
	})
	if *input == "" || *output == "" || flags.NArg() != 0 || !explicitLimit || (*primary != cold && *primary != noChange) ||
		math.IsNaN(*limit) || math.IsInf(*limit, 0) || *limit <= 0 || *limit >= 1 ||
		*coldCount < 10 || *coldCount%2 != 0 || *noChangeCount < 20 || *noChangeCount%2 != 0 {
		fmt.Fprintln(stderr, "usage: perfaudit --input manifest.json --output new-audit.json --primary cold|no_change --max-primary-ratio 0.95 [--cold-pairs 30 --no-change-pairs 100]; ratio must be finite in (0,1), counts even and >=10/20")
		return 2
	}
	data, err := os.ReadFile(*input)
	if err != nil {
		fmt.Fprintf(stderr, "read input: %v\n", err)
		return 2
	}
	// Reserve output before doing potentially expensive bootstrap work. O_EXCL
	// also prevents alias/hardlink/symlink paths from overwriting raw evidence.
	f, err := os.OpenFile(*output, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o644)
	if err != nil {
		fmt.Fprintf(stderr, "create output: %v\n", err)
		return 2
	}
	r := evaluate(data, config{*primary, *limit, *coldCount, *noChangeCount})
	encoder := json.NewEncoder(f)
	encoder.SetIndent("", "  ")
	encodeErr := encoder.Encode(r)
	closeErr := f.Close()
	if encodeErr != nil || closeErr != nil {
		fmt.Fprintf(stderr, "write output: encode=%v close=%v (partial output retained)\n", encodeErr, closeErr)
		return 2
	}
	printSummary(stdout, r)
	if r.Outcome == criterionMet {
		return 0
	}
	return 1
}

func printSummary(w io.Writer, r report) {
	fmt.Fprintf(w, "%s (recorded-manifest audit only; promotion_ready=false)\n", r.Outcome)
	for _, phase := range []string{cold, noChange} {
		s, ok := r.Statistics[phase]
		if !ok {
			continue
		}
		e := s.Estimates
		fmt.Fprintf(w, "%s: n=%d blocks=%d; median B/C %.6g/%.6g ms; p95 B/C %.6g/%.6g ms; paired difference %.6g ms; paired ratio %.6g [%.6g, %.6g]; p95 ratio %.6g [%.6g, %.6g]\n",
			phase, s.PairCount, s.BlockCount, e.BaselineMedianMS.Value, e.CandidateMedianMS.Value,
			e.BaselineP95MS.Value, e.CandidateP95MS.Value, e.PairedDifferenceMS.Value,
			e.PairedRatio.Value, e.PairedRatio.CI95.Lower, e.PairedRatio.CI95.Upper,
			e.P95Ratio.Value, e.P95Ratio.CI95.Lower, e.P95Ratio.CI95.Upper)
	}
	if r.Primary != nil {
		fmt.Fprintf(w, "primary %s (ratio <= %.6g, CI upper < 1): %s; no_change guardrail (CI upper <= %.6g): %s\n",
			r.Config.Primary, r.Config.MaxPrimaryRatio, r.Primary.Status, r.GuardrailLimit, r.Guardrail.Status)
	}
	for _, problem := range r.Issues {
		fmt.Fprintf(w, "%s %s: %s\n", problem.Kind, problem.Path, problem.Message)
	}
	fmt.Fprintln(w, "External gates unverified: harness completion, runtime provenance/quiet host, semantic/write parity, A/A noise, independent confirmation, resources, preregistration/review. Legacy verdict required separately; cold<=0.60/no_change<=1.10 remains unchanged.")
}
