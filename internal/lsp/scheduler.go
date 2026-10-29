package lsp

import (
	"sync"
	"time"
)

// scheduler decides when a census runs. A census is a full cold compile, so the
// goal is the fewest runs that still leave the editor showing the latest text:
//
//   - at most one run per tsconfig at a time;
//   - edits arriving while a run is in flight mark the config dirty for exactly
//     one more run, however many edits there were (coalesced, never queued);
//   - typing is debounced, while opening and saving a file run immediately.
type scheduler struct {
	debounce time.Duration
	run      func(config string)

	mu      sync.Mutex
	closed  bool
	configs map[string]*configState
	wg      sync.WaitGroup
}

// configState is the scheduling state of one tsconfig.
type configState struct {
	// dirty means the config's inputs changed since its last run started.
	dirty bool
	// running means a goroutine is looping over this config's runs.
	running bool
	// timer is the pending debounce, if any.
	timer *time.Timer
}

func newScheduler(debounce time.Duration, run func(config string)) *scheduler {
	return &scheduler{
		debounce: debounce,
		run:      run,
		configs:  map[string]*configState{},
	}
}

// schedule marks config dirty and arranges a run: now when immediate, after the
// debounce otherwise. If a run is already in flight it will notice the dirty
// flag when it finishes, so nothing else is needed.
func (s *scheduler) schedule(config string, immediate bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return
	}
	state := s.configs[config]
	if state == nil {
		state = &configState{}
		s.configs[config] = state
	}
	state.dirty = true
	if state.running {
		return
	}
	if immediate {
		if state.timer != nil {
			state.timer.Stop()
		}
		s.start(state, config)
		return
	}
	if state.timer == nil {
		state.timer = time.AfterFunc(s.debounce, func() { s.fire(config) })
	} else {
		state.timer.Reset(s.debounce)
	}
}

// fire is the debounce timer's callback. The config may have been run in the
// meantime (an immediate request overtook the timer), in which case there is
// nothing left to do.
func (s *scheduler) fire(config string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	state := s.configs[config]
	if s.closed || state == nil || state.running || !state.dirty {
		return
	}
	s.start(state, config)
}

// start launches the run loop for a config. The caller holds mu.
func (s *scheduler) start(state *configState, config string) {
	state.running = true
	s.wg.Add(1)
	go func() {
		defer s.wg.Done()
		for {
			s.mu.Lock()
			if s.closed || !state.dirty {
				state.running = false
				s.mu.Unlock()
				return
			}
			state.dirty = false
			s.mu.Unlock()
			s.run(config)
		}
	}()
}

// close stops scheduling and waits for in-flight runs to return. The caller
// cancels the context the runs observe first, or this waits out a full census.
func (s *scheduler) close() {
	s.mu.Lock()
	s.closed = true
	for _, state := range s.configs {
		if state.timer != nil {
			state.timer.Stop()
		}
	}
	s.mu.Unlock()
	s.wg.Wait()
}
