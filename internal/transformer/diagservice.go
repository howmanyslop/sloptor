package transformer

// DiagService is the Go port of TSTransformer/classes/DiagnosticService.ts.
// Upstream is a global static accumulator; rotor injects an instance instead
// so concurrent compilations stay independent.
type DiagService struct {
	diagnostics []Diagnostic
	// single tracks codes already reported via AddSingle (upstream
	// singleDiagnostics: Set<number> keyed by factory id; we key by the
	// stable factory name).
	single map[string]bool
}

// NewDiagService returns an empty diagnostic accumulator.
func NewDiagService() *DiagService {
	return &DiagService{single: make(map[string]bool)}
}

// Add appends a diagnostic unconditionally (upstream addDiagnostic).
func (s *DiagService) Add(d Diagnostic) {
	s.diagnostics = append(s.diagnostics, d)
}

// AddSingle appends a diagnostic unless one with the same Code was already
// added via AddSingle since the last Flush (upstream addSingleDiagnostic).
func (s *DiagService) AddSingle(d Diagnostic) {
	if s.single[d.Code] {
		return
	}
	s.single[d.Code] = true
	s.Add(d)
}

// AddDiagnosticWithCache appends d unless key is already present in the
// caller-provided cache, recording key on first use (upstream
// addDiagnosticWithCache). A free function because Go methods cannot have
// type parameters.
func AddDiagnosticWithCache[K comparable](s *DiagService, key K, d Diagnostic, cache map[K]bool) {
	if cache[key] {
		return
	}
	cache[key] = true
	s.Add(d)
}

// Flush returns the accumulated diagnostics and resets the service, including
// the AddSingle dedupe set (upstream flush).
func (s *DiagService) Flush() []Diagnostic {
	out := s.diagnostics
	s.diagnostics = nil
	s.single = make(map[string]bool)
	return out
}

// HasErrors reports whether any currently-pending diagnostic is an error
// (upstream hasErrors: any with category Error).
func (s *DiagService) HasErrors() bool {
	for _, d := range s.diagnostics {
		if !d.Warning {
			return true
		}
	}
	return false
}
