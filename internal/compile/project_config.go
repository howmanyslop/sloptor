package compile

import (
	"path/filepath"

	"rotor/tsgo/core"
	"rotor/tsgo/outputpaths"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/tspath"
)

// projectOutputHost is the part of a program that output path translation
// reads. Both *compiler.Program and *projectConfig satisfy it.
type projectOutputHost interface {
	outputpaths.OutputPathsHost
	Options() *core.CompilerOptions
}

// projectConfig is a parsed project config: the options and root file names
// a program would be built from, without building the program. Output paths
// depend only on these, because a config file always fixes the common source
// directory (rootDir, else the config directory).
type projectConfig struct {
	parsed           *tsoptions.ParsedCommandLine
	currentDirectory string
	caseSensitive    bool
}

// parseProjectConfig parses configPath the way newProjectProgram does.
func parseProjectConfig(configPath string) (*projectConfig, []string, error) {
	absolute, err := filepath.Abs(configPath)
	if err != nil {
		return nil, nil, err
	}
	configPath = filepath.ToSlash(absolute)
	dir := tspath.GetDirectoryPath(configPath)
	fs := solutionCacheFS(nil, configPath, nil)
	_, parsed, diagnostics, err := parseProjectCommandLine(dir, configPath, fs, nil, nil, nil, nil)
	if err != nil {
		return nil, diagnostics, err
	}
	return &projectConfig{parsed: parsed, currentDirectory: dir, caseSensitive: fs.UseCaseSensitiveFileNames()}, nil, nil
}

func (c *projectConfig) Options() *core.CompilerOptions { return c.parsed.CompilerOptions() }
func (c *projectConfig) FileNames() []string            { return c.parsed.FileNames() }
func (c *projectConfig) GetCurrentDirectory() string    { return c.currentDirectory }
func (c *projectConfig) UseCaseSensitiveFileNames() bool {
	return c.caseSensitive
}

func (c *projectConfig) CommonSourceDirectory() string {
	return outputpaths.GetCommonSourceDirectory(c.Options(), func() []string {
		files := []string{}
		for _, fileName := range c.FileNames() {
			if !tspath.IsDeclarationFileName(fileName) {
				files = append(files, fileName)
			}
		}
		return files
	}, c.currentDirectory, c.caseSensitive, nil)
}
