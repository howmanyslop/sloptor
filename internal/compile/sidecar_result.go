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
	request  func(sidecarRequest) (*sidecarResponse, error)
	released bool
}

func newSidecarTraceLease(handle string, request func(sidecarRequest) (*sidecarResponse, error)) *sidecarTraceLease {
	if handle == "" || request == nil {
		return nil
	}
	return &sidecarTraceLease{handle: handle, maps: make(map[string]string), request: request}
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
	response, err := l.request(sidecarRequest{
		Protocol:     sidecarNodeProtocolVersion,
		Operation:    "maps",
		ResultHandle: l.handle,
		FileNames:    []string{fileName},
	})
	if err != nil {
		return "", err
	}
	if err := sidecarControlDiagnostics(response); err != nil {
		return "", err
	}
	for _, trace := range response.TraceMaps {
		l.maps[normalizeSourceFilePath(trace.FileName)] = trace.TraceMap
	}
	if len(response.TraceMaps) == 1 {
		l.maps[key] = response.TraceMaps[0].TraceMap
	}
	raw, ok := l.maps[key]
	if !ok {
		return "", fmt.Errorf("transformer result did not return a trace map for %s", fileName)
	}
	return raw, nil
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
	response, err := l.request(sidecarRequest{
		Protocol:     sidecarNodeProtocolVersion,
		Operation:    "release",
		ResultHandle: l.handle,
		Outcome:      outcome,
	})
	if err == nil {
		err = sidecarControlDiagnostics(response)
	}
	if err == nil {
		l.released = true
		l.maps = nil
	}
	return err
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
