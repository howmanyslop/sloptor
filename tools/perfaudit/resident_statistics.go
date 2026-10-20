package main

import (
	"crypto/sha256"
	"encoding/binary"
	"fmt"
	"math"
	"slices"
)

// At most 15 blocks (5+10). Each replicate stores block multiplicities, not
// independent cycle indices. All estimands below consume this one table.
type residentDrawPlan struct {
	weights [][15]uint8
	first   [][]int
	sha     string
}

func residentPlan(counts []int) residentDrawPlan {
	rng := splitMix64{bootstrapSeed}
	p := residentDrawPlan{weights: make([][15]uint8, bootstrapReplicates)}
	h := sha256.New()
	var encoded [4]byte
	for draw := range p.weights {
		offset := 0
		var first []int
		for _, n := range counts {
			for range n {
				local := rng.index(n)
				p.weights[draw][offset+local]++
				binary.LittleEndian.PutUint32(encoded[:], uint32(local))
				_, _ = h.Write(encoded[:])
				if draw < 2 {
					first = append(first, offset+local)
				}
			}
			offset += n
		}
		if draw < 2 {
			p.first = append(p.first, first)
		}
	}
	p.sha = fmt.Sprintf("%x", h.Sum(nil))
	return p
}

type residentLogValue struct {
	value float64
	block int
	order int
}

type ResidentBenefit struct {
	Log     estimate `json:"log"`
	Ratio   estimate `json:"ratio"`
	Reverse estimate `json:"reverse_ratio"`
}

func residentLogMedian(values []residentLogValue, weights [15]uint8, order int) float64 {
	n := 0
	for _, v := range values {
		if order == -1 || order == v.order {
			n += int(weights[v.block])
		}
	}
	left, right, cumulative := (n-1)/2, n/2, 0
	var a, b float64
	for _, v := range values {
		if order != -1 && order != v.order {
			continue
		}
		previous := cumulative
		cumulative += int(weights[v.block])
		if previous <= left && cumulative > left {
			a = v.value
		}
		if previous <= right && cumulative > right {
			b = v.value
			break
		}
	}
	return a/2 + b/2
}

func residentBenefitFor(blocks []ResidentBlock, plan residentDrawPlan, order int) ResidentBenefit {
	values := make([]residentLogValue, 0, len(blocks)*60)
	for b, block := range blocks {
		for o := range 2 {
			a, c := block.Sessions[0], block.Sessions[1]
			if o == 1 {
				a, c = block.Sessions[3], block.Sessions[2]
			}
			for j := range 30 {
				// Deliberately approximate transcendental estimand; no epsilon or
				// boundary snapping. Native ns stay exact in marginal controls.
				v := math.Log(float64(c.Cycles[j].Nanoseconds) / float64(a.Cycles[j].Nanoseconds))
				values = append(values, residentLogValue{v, b, o})
			}
		}
	}
	slices.SortFunc(values, func(a, b residentLogValue) int {
		if a.value < b.value {
			return -1
		}
		if a.value > b.value {
			return 1
		}
		return 0
	})
	var ones [15]uint8
	for i := range blocks {
		ones[i] = 1
	}
	draws := make([]float64, bootstrapReplicates)
	cache := map[[15]uint8]float64{}
	for i, weights := range plan.weights {
		v, ok := cache[weights]
		if !ok {
			v = residentLogMedian(values, weights, order)
			cache[weights] = v
		}
		draws[i] = v
	}
	log := resourceEstimate(residentLogMedian(values, ones, order), draws)
	return ResidentBenefit{
		Log:     log,
		Ratio:   estimate{math.Exp(log.Value), interval{math.Exp(log.CI95.Lower), math.Exp(log.CI95.Upper)}},
		Reverse: estimate{math.Exp(-log.Value), interval{math.Exp(-log.CI95.Upper), math.Exp(-log.CI95.Lower)}},
	}
}

type ResidentPopulation struct {
	CyclePairs   int                       `json:"cycle_pairs"`
	SessionPairs int                       `json:"session_pairs"`
	Benefit      ResidentBenefit           `json:"benefit"`
	Metrics      map[string]ResidentMetric `json:"metrics"`
	Status       string                    `json:"control_status"`
}

type ResidentSummary struct {
	Status         string                        `json:"status"`
	Issues         []issue                       `json:"issues"`
	Batches        []ResidentBatch               `json:"typed_batches"`
	BlockCounts    []int                         `json:"block_counts"`
	Replicates     int                           `json:"replicates"`
	Seed           uint64                        `json:"seed"`
	DrawSHA256     string                        `json:"draw_indices_sha256"`
	FirstDraws     [][]int                       `json:"first_draws_global_indices"`
	Populations    map[string]ResidentPopulation `json:"populations"`
	Sessions       []ResidentSessionDistribution `json:"session_distributions"`
	Noise          float64                       `json:"noise"`
	PromotionReady bool                          `json:"promotion_ready"`
}

// AnalyzeResident is the stable in-package seam for the later reviewed reader.
// It accepts one complete initial/follow-up/confirmation, or initial+follow-up.
// It never reads a file, launches a process, or validates raw clock/provenance.
func AnalyzeResident(batches []ResidentBatch) ResidentSummary {
	r := ResidentSummary{Batches: batches, Status: criterionMet, Seed: bootstrapSeed, Replicates: bootstrapReplicates}
	if len(batches) < 1 || len(batches) > 2 {
		r.Issues = append(r.Issues, issue{invalidEvidence, "batches", "one batch or initial+follow_up required"})
	} else {
		for _, b := range batches {
			r.Issues = append(r.Issues, residentTypedIssues(b)...)
		}
		if len(batches) == 2 {
			a, b := batches[0], batches[1]
			if a.Stage != "initial" || b.Stage != "follow_up" || a.Key != b.Key || a.Purpose != b.Purpose || a.Chronology >= b.Chronology || a.ID == b.ID {
				r.Issues = append(r.Issues, issue{invalidEvidence, "batches", "joint analysis requires chronological matching initial+follow_up strata"})
			}
		}
		r.Issues = append(r.Issues, residentDuplicateSessions(batches)...)
	}
	r.Status = residentIssueStatus(r.Issues)
	if r.Status != criterionMet {
		return r
	}
	var blocks []ResidentBlock
	for _, b := range batches {
		r.BlockCounts = append(r.BlockCounts, len(b.Blocks))
		blocks = append(blocks, b.Blocks...)
	}
	plan := residentPlan(r.BlockCounts)
	r.DrawSHA256, r.FirstDraws = plan.sha, plan.first
	r.Populations = map[string]ResidentPopulation{}
	r.Sessions = residentSessionDistributions(blocks)
	for order, name := range []string{"full", "AB", "BA"} {
		stratum := order - 1
		pairs := 2 * len(blocks)
		if stratum != -1 {
			pairs /= 2
		}
		p := ResidentPopulation{SessionPairs: pairs, CyclePairs: pairs * 30, Benefit: residentBenefitFor(blocks, plan, stratum), Metrics: map[string]ResidentMetric{}, Status: criterionMet}
		for _, metric := range residentMetricNames {
			m := residentMetricFor(blocks, plan, stratum, metric, batches[0].Key, batches[0].Purpose == "calibration")
			p.Metrics[metric] = m
			// Order controls are diagnostic; the full population decides the
			// leaf. Full/order log points and endpoints all contribute to N.
			p.Status = resourceStatuses(p.Status, m.Status)
		}
		r.Populations[name] = p
		log := p.Benefit.Log
		r.Noise = math.Max(r.Noise, math.Max(math.Abs(log.Value), math.Max(math.Abs(log.CI95.Lower), math.Abs(log.CI95.Upper))))
		if stratum == -1 {
			r.Status = resourceStatuses(r.Status, p.Status)
		}
	}
	return r
}

func residentDuplicateSessions(batches []ResidentBatch) []issue {
	seen := map[string]bool{}
	for _, b := range batches {
		if len(b.Blocks) > 10 {
			continue
		}
		for _, block := range b.Blocks {
			if len(block.Sessions) > 4 {
				continue
			}
			for _, s := range block.Sessions {
				if seen[s.ID] {
					return []issue{{invalidEvidence, "sessions", "session reused across supplied stages/populations"}}
				}
				seen[s.ID] = true
			}
		}
	}
	return nil
}
