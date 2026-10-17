package deploy

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

// stateVersion is the on-disk schema version of environment state files.
const stateVersion = 1

// StateEntry is one resource's persisted record: the canonical hash of the
// inputs it was last applied with, and the outputs that apply produced.
type StateEntry struct {
	InputsHash string         `json:"inputsHash"`
	Outputs    map[string]any `json:"outputs,omitempty"`
}

// State is an environment's deployment state, persisted to
// .rotor/deploy/<env>.json. Keys are "<kind>/<name>".
type State struct {
	Version   int                    `json:"version"`
	Resources map[string]*StateEntry `json:"resources"`
}

// NewState returns an empty state at the current schema version.
func NewState() *State {
	return &State{Version: stateVersion, Resources: map[string]*StateEntry{}}
}

// StatePath returns the state-file location for an environment under a
// project root: <projectDir>/.rotor/deploy/<env>.json.
func StatePath(projectDir, env string) string {
	return filepath.Join(projectDir, ".rotor", "deploy", env+".json")
}

// LoadState reads a state file, returning an empty state when the file does
// not exist (first deploy).
func LoadState(path string) (*State, error) {
	data, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return NewState(), nil
	}
	if err != nil {
		return nil, fmt.Errorf("deploy: reading state %s: %w", path, err)
	}
	st := &State{}
	if err := json.Unmarshal(data, st); err != nil {
		return nil, fmt.Errorf("deploy: parsing state %s: %w", path, err)
	}
	if st.Version != stateVersion {
		return nil, fmt.Errorf("deploy: state %s has version %d; this rotor understands version %d", path, st.Version, stateVersion)
	}
	if st.Resources == nil {
		st.Resources = map[string]*StateEntry{}
	}
	return st, nil
}

// Save writes the state atomically: marshal to a temp file in the target
// directory, then rename over the destination. Called after every applied
// resource so an interrupted apply resumes where it left off.
func (s *State) Save(path string) error {
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return fmt.Errorf("deploy: creating state dir: %w", err)
	}
	data, err := json.MarshalIndent(s, "", "  ")
	if err != nil {
		return fmt.Errorf("deploy: encoding state: %w", err)
	}
	data = append(data, '\n')

	tmp, err := os.CreateTemp(dir, ".state-*.tmp")
	if err != nil {
		return fmt.Errorf("deploy: writing state: %w", err)
	}
	tmpName := tmp.Name()
	_, err = tmp.Write(data)
	if closeErr := tmp.Close(); err == nil {
		err = closeErr
	}
	if err == nil {
		err = os.Rename(tmpName, path)
	}
	if err != nil {
		_ = os.Remove(tmpName) // best-effort; the write error is what the caller needs
		return fmt.Errorf("deploy: writing state: %w", err)
	}
	return nil
}
