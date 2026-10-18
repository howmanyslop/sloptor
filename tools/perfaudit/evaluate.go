package main

import (
	"crypto/sha256"
	"fmt"
)

const (
	invalidEvidence    = "invalid-evidence"
	correctnessFailure = "correctness-failure"
	inconclusive       = "inconclusive"
	criterionMet       = "criterion-met"
	criterionNotMet    = "criterion-not-met"
	guardrailLimit     = 1.10
)

type config struct {
	Primary         string  `json:"primary_phase"`
	MaxPrimaryRatio float64 `json:"max_primary_ratio"`
	ColdPairs       int     `json:"cold_pairs"`
	NoChangePairs   int     `json:"no_change_pairs"`
}

type decision struct {
	Status  string `json:"status"`
	Rule    string `json:"rule"`
	Message string `json:"message"`
}

type externalGate struct {
	Name        string `json:"name"`
	Status      string `json:"status"`
	Requirement string `json:"requirement"`
}

type method struct {
	Algorithm           string `json:"prng_algorithm"`
	Seed                uint64 `json:"seed_per_phase"`
	Replicates          int    `json:"replicates"`
	Block               string `json:"block"`
	Endpoints           string `json:"endpoints"`
	RatioTransformation string `json:"paired_ratio_transformation"`
}

type rawInput struct {
	SHA256 string `json:"sha256"`
	Bytes  []byte `json:"raw_json_base64"`
}

type report struct {
	Schema         int                        `json:"schema"`
	Outcome        string                     `json:"outcome"`
	Scope          string                     `json:"scope"`
	Input          rawInput                   `json:"input"`
	Config         config                     `json:"config"`
	GuardrailLimit float64                    `json:"no_change_p95_ratio_limit"`
	Bootstrap      method                     `json:"bootstrap"`
	Issues         []issue                    `json:"issues"`
	Statistics     map[string]phaseStatistics `json:"statistics,omitempty"`
	Primary        *decision                  `json:"primary_decision,omitempty"`
	Guardrail      *decision                  `json:"guardrail_decision,omitempty"`
	ExternalGates  []externalGate             `json:"external_gates"`
	Limitations    []string                   `json:"limitations"`
	PromotionReady bool                       `json:"promotion_ready"`
	LegacyVerdict  string                     `json:"legacy_verdict"`
}

func evaluate(input []byte, cfg config) report {
	phases, issues := validate(input, cfg)
	r := report{
		Schema: 1, Outcome: invalidEvidence,
		Scope:  "schema1 manifest statistics and recorded pair correctness only",
		Input:  rawInput{fmt.Sprintf("%x", sha256.Sum256(input)), input},
		Config: cfg, GuardrailLimit: guardrailLimit, Issues: issues,
		Bootstrap: method{
			Algorithm: "SplitMix64; uint64 wrapping; rejection before modulo for uniform block indices",
			Seed:      bootstrapSeed, Replicates: bootstrapReplicates,
			Block:               "two adjacent opposite-order pairs (four commands); sample blocks with replacement within phase; same selected blocks for both arms",
			Endpoints:           "central 95%; sorted zero-based h=(20000-1)*p, linear interpolation at p=0.025 and 0.975 (type 7)",
			RatioTransformation: "exp(median(log(candidate/baseline))); CI endpoints are exp of interpolated log-median endpoints",
		},
		LegacyVerdict: "unverified: run unchanged tools/perfcompare separately and retain its actual verdict (cold median <=0.60; no_change p95 <=1.10)",
		ExternalGates: []externalGate{
			{"harness_completion", "unverified", "Review companion run.json status=evaluated AND final published manifest AND actual legacy verdict; reject pending/incomplete/cleanup-failed runs. A manifest alone cannot establish clean completion."},
			{"runtime_provenance_and_quiet_host", "unverified", "Review binary/source/fixture/dependency identities, invocation journal/logs, measurement boundary, clean input snapshots and root host lease; supplied labels are not runtime proof."},
			{"semantic_and_write_parity", "unverified", "Review independent oracles, all contract artifacts, mtimes and actual writes; equal limited-scope digests establish only recorded equality."},
			{"aa_noise_floor", "unverified", "Supply separately reviewed balanced-path A/A calibration and evidence that the preregistered practical effect exceeds noise."},
			{"independent_confirmation", "unverified", "Supply separately reviewed independent primary and second-workload confirmation; apply the predeclared fixed follow-up policy without optional stopping."},
			{"resource_regressions", "unverified", "Review CPU, memory and I/O evidence; elapsed records do not measure them."},
			{"preregistration_and_evaluator_review", "unverified", "Review the pre-score workload/phase/threshold/count declaration and independent statistical/evidence review of this exact evaluator SHA before promotion gating."},
		},
		Limitations: []string{
			"Finite-sample percentile block bootstrap is descriptive uncertainty, not an exact-coverage promise; few blocks and marginal p95 tails can be unstable.",
			"Adjacent AB/BA blocks preserve short-range dependence and order strata; dependence across blocks, drift and path effects may remain.",
			"Schema1 integer milliseconds may be rounded; float64 statistics may additionally lose precision for very large durations. Tiny effects need better measurement resolution.",
			"Degenerate bootstrap distributions yield deterministic equal endpoints; zero width does not establish zero real-world noise.",
			"No outliers or failed records are removed. Any malformed evidence suppresses statistics; all discoverable issues and the exact input bytes are retained.",
			"Identity checks cover nonempty schema1 metadata, recognized hash syntax and declared output-digest format. Unknown extensions are retained but not interpreted; identity labels do not prove provenance.",
		},
	}
	for _, problem := range issues {
		if problem.Kind == invalidEvidence {
			return r
		}
	}
	r.Statistics = map[string]phaseStatistics{}
	for _, phase := range []string{cold, noChange} {
		r.Statistics[phase] = summarize(phases[phase])
	}
	primary := primaryDecision(r.Statistics[cfg.Primary].Estimates.PairedRatio, cfg.MaxPrimaryRatio)
	guardrail := guardrailDecision(r.Statistics[noChange].Estimates.P95Ratio)
	r.Primary, r.Guardrail = &primary, &guardrail
	r.Outcome = criterionMet
	if primary.Status == inconclusive || guardrail.Status == inconclusive {
		r.Outcome = inconclusive
	}
	if primary.Status == criterionNotMet || guardrail.Status == criterionNotMet {
		r.Outcome = criterionNotMet
	}
	if len(issues) > 0 {
		r.Outcome = correctnessFailure
	}
	return r
}

func primaryDecision(ratio estimate, limit float64) decision {
	d := decision{Rule: fmt.Sprintf("paired ratio point <= %.17g AND 95%% CI upper < 1", limit)}
	switch {
	case ratio.CI95.Lower >= 1:
		d.Status, d.Message = criterionNotMet, "interval does not support improvement"
	case ratio.CI95.Upper >= 1:
		d.Status, d.Message = inconclusive, "interval includes no improvement"
	case ratio.Value > limit:
		d.Status, d.Message = criterionNotMet, "benefit is separated from zero but point estimate misses declared practical threshold"
	default:
		d.Status, d.Message = criterionMet, "point practical threshold and uncertainty requirement both met"
	}
	return d
}

func guardrailDecision(ratio estimate) decision {
	d := decision{Rule: "no_change marginal p95 candidate/baseline ratio 95% CI upper <= 1.10"}
	switch {
	case ratio.CI95.Upper <= guardrailLimit:
		d.Status, d.Message = criterionMet, "interval is within guardrail"
	case ratio.CI95.Lower > guardrailLimit:
		d.Status, d.Message = criterionNotMet, "entire interval exceeds guardrail"
	default:
		d.Status, d.Message = inconclusive, "guardrail interval crosses the limit"
	}
	return d
}
