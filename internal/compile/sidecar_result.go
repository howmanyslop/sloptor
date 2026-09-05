package compile

import (
	"errors"
	"fmt"
	"sync"

	"rotor/internal/logservice"
)

type sidecarTraceLease struct {
	mu       sync.Mutex
	handle   string
	maps     map[string]string
	files    map[string]string
	aliases  map[string][]string
	request  func(sidecarRequest) (*sidecarResponse, sidecarCallStats, error)
	stats    sidecarCallStats
	released bool
}

func newSidecarTraceLease(handle string, request func(sidecarRequest) (*sidecarResponse, sidecarCallStats, error)) *sidecarTraceLease {
	if handle == "" || request == nil {
		return nil
	}
	return &sidecarTraceLease{
		handle:  handle,
		maps:    make(map[string]string),
		files:   make(map[string]string),
		aliases: make(map[string][]string),
		request: request,
	}
}

func (l *sidecarTraceLease) rememberFile(fileName, workerFileName string) {
	if l == nil {
		return
	}
	l.mu.Lock()
	key := normalizeSourceFilePath(fileName)
	workerKey := normalizeSourceFilePath(workerFileName)
	l.files[key] = workerFileName
	l.aliases[workerKey] = append(l.aliases[workerKey], key)
	l.mu.Unlock()
}

func (l *sidecarTraceLease) traceMap(fileName string) (string, error) {
	key := normalizeSourceFilePath(fileName)
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.released {
		return "", errors.New("transformer result was released before its trace map was requested")
	}
	if raw, ok := l.maps[key]; ok {
		return raw, nil
	}
	workerFileName, ok := l.files[key]
	if !ok {
		return "", fmt.Errorf("transformer result has no retained file for %s", fileName)
	}
	if err := l.fetchMapsLocked([]string{workerFileName}); err != nil {
		return "", err
	}
	raw, ok := l.maps[key]
	if !ok {
		return "", fmt.Errorf("transformer result did not return a trace map for %s", fileName)
	}
	return raw, nil
}

func (l *sidecarTraceLease) prefetchTraceMaps(fileNames []string) error {
	if l == nil || len(fileNames) == 0 {
		return nil
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.released {
		return errors.New("transformer result was released before its trace map was requested")
	}
	requested := make([]string, 0, len(fileNames))
	seen := make(map[string]struct{}, len(fileNames))
	for _, fileName := range fileNames {
		key := normalizeSourceFilePath(fileName)
		actual, ok := l.files[key]
		if !ok {
			continue
		}
		if _, ok := l.maps[key]; ok {
			continue
		}
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		requested = append(requested, actual)
	}
	return l.fetchMapsLocked(requested)
}

func (l *sidecarTraceLease) fetchMapsLocked(fileNames []string) error {
	if len(fileNames) == 0 {
		return nil
	}
	response, stats, err := l.request(sidecarRequest{
		Protocol:     sidecarNodeProtocolVersion,
		Operation:    "maps",
		ResultHandle: l.handle,
		FileNames:    fileNames,
	})
	l.stats.add(stats)
	if err != nil {
		return err
	}
	if err := sidecarControlDiagnostics(response); err != nil {
		return err
	}
	for _, trace := range response.TraceMaps {
		workerKey := normalizeSourceFilePath(trace.FileName)
		l.maps[workerKey] = trace.TraceMap
		for _, alias := range l.aliases[workerKey] {
			l.maps[alias] = trace.TraceMap
		}
	}
	for _, fileName := range fileNames {
		if _, ok := l.maps[normalizeSourceFilePath(fileName)]; !ok {
			return fmt.Errorf("transformer result did not return a trace map for %s", fileName)
		}
	}
	return nil
}

func (l *sidecarTraceLease) release(outcome string) error {
	if l == nil {
		return nil
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.released {
		return nil
	}
	response, stats, err := l.request(sidecarRequest{
		Protocol:     sidecarNodeProtocolVersion,
		Operation:    "release",
		ResultHandle: l.handle,
		Outcome:      outcome,
	})
	l.stats.add(stats)
	if err == nil {
		err = sidecarControlDiagnostics(response)
	}
	if err == nil {
		l.released = true
		l.maps = nil
	}
	return err
}

func (l *sidecarTraceLease) takeStats() sidecarCallStats {
	if l == nil {
		return sidecarCallStats{}
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	stats := l.stats
	l.stats = sidecarCallStats{}
	return stats
}

func prefetchSidecarTraceMaps(leases []*sidecarTraceLease, fileNames []string) error {
	for _, lease := range leases {
		if err := lease.prefetchTraceMaps(fileNames); err != nil {
			return err
		}
	}
	return nil
}

func takeSidecarTraceLeaseStats(leases []*sidecarTraceLease) sidecarCallStats {
	var stats sidecarCallStats
	for _, lease := range leases {
		stats.add(lease.takeStats())
	}
	return stats
}

func sidecarControlDiagnostics(response *sidecarResponse) error {
	if response == nil {
		return errors.New("transformer worker returned no control response")
	}
	for _, diagnostic := range response.Diagnostics {
		if diagnostic.Category != "warning" {
			return errors.New(diagnostic.Message)
		}
	}
	return nil
}

func releaseSidecarTraceLeases(leases []*sidecarTraceLease, outcome string) {
	for _, lease := range leases {
		if err := lease.release(outcome); err != nil {
			logservice.Warn("release transformer result: " + err.Error())
		}
	}
}
