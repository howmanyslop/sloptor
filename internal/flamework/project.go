package flamework

import (
	"errors"
	"fmt"
	"path/filepath"
	"strings"

	"rotor/internal/config"
	"rotor/internal/rojo"
	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/diagnostics"
)

var (
	ErrPackageNotFound = errors.New("flamework package.json not found")
	ErrInvalidProject  = errors.New("invalid Flamework project")
)

type upstreamBoundaryError string

func (e upstreamBoundaryError) Error() string {
	return string(e)
}

type IDMode string

type ProjectDiagnosticError struct {
	diagnostics []*ast.Diagnostic
}

func (e *ProjectDiagnosticError) Error() string {
	messages := make([]string, len(e.diagnostics))
	for index, diagnostic := range e.diagnostics {
		messages[index] = diagnostic.String()
	}
	return strings.Join(messages, "\n")
}

func (e *ProjectDiagnosticError) Diagnostics() []*ast.Diagnostic {
	return append([]*ast.Diagnostic(nil), e.diagnostics...)
}

const (
	IDModeFull       IDMode = "full"
	IDModeObfuscated IDMode = "obfuscated"
	IDModeShort      IDMode = "short"
	IDModeTiny       IDMode = "tiny"
)

type ProjectOptions struct {
	ProjectDir       string
	RootDir          string
	OutDir           string
	IncludeDirectory string
	RojoConfigPath   string
	Declaration      bool
	Config           config.FlameworkConfig
}

type Project struct {
	projectDirectory string
	rootDirectory    string
	outDirectory     string
	includeDirectory string
	packageName      string
	packageVersion   string
	hashPrefix       string
	idMode           IDMode
	isGame           bool
	packMode         bool
	config           config.FlameworkConfig
	translator       *rojo.PathTranslator
	resolver         *rojo.RojoResolver
	analysis         *AnalysisState
	buildInfo        *BuildInfo
	runtimeConfig    *RuntimeConfig
}

func OpenProject(options ProjectOptions) (*Project, error) {
	projectRoot, err := openProjectRoot(options.ProjectDir)
	if err != nil {
		return nil, err
	}
	projectDirectory := projectRoot.directory

	if options.RootDir == "" {
		return nil, upstreamBoundaryError("Assertion Failed! rootDir or rootDirs must be specified\nPlease submit a bug report here:\nhttps://github.com/rbxts-flamework/core/issues")
	}
	rootDirectory, err := resolveProjectPath(projectDirectory, options.RootDir, ".")
	if err != nil {
		return nil, err
	}
	outDirectory, err := resolveProjectPath(projectDirectory, options.OutDir, ".")
	if err != nil {
		return nil, err
	}
	includeDirectory, err := resolveProjectPath(projectDirectory, options.IncludeDirectory, "include")
	if err != nil {
		return nil, err
	}

	mode := IDMode(options.Config.IDGenerationMode)
	if mode == "" {
		mode = IDModeFull
		if options.Config.Obfuscation {
			mode = IDModeObfuscated
		}
	}
	if !mode.valid() {
		return nil, fmt.Errorf("%w: ID generation mode %q", ErrInvalidProject, mode)
	}

	isGame := !strings.HasPrefix(projectRoot.packageName, "@")
	packMode := projectRoot.packageName == "@flamework/core"
	hashPrefix := options.Config.HashPrefix
	if !isGame && hashPrefix == "" {
		hashPrefix = projectRoot.packageName
	}
	if strings.HasPrefix(hashPrefix, "$") && !packMode {
		return nil, upstreamBoundaryError("The hashPrefix $ is used internally by Flamework")
	}

	translator := rojo.NewPathTranslator(rootDirectory, outDirectory, "", options.Declaration, false)
	resolver, err := openRojoResolver(projectDirectory, outDirectory, options.RojoConfigPath)
	if err != nil {
		return nil, err
	}
	buildInfo, err := loadProjectBuildInfo(projectDirectory, projectRoot.packageDirectory)
	if err != nil {
		return nil, err
	}
	if options.Config.Salt != "" {
		buildInfo.SetSalt(nil)
	}
	if previous := buildInfo.FlameworkVersion(); previous != FlameworkVersion {
		relativeOut, relativeErr := filepath.Rel(projectDirectory, outDirectory)
		if relativeErr != nil {
			relativeOut = outDirectory
		}
		return nil, newProjectDiagnosticError(
			"Project was compiled on different version of Flamework.",
			"Please recompile by deleting the "+filepath.ToSlash(relativeOut)+" directory",
			"Current Flamework Version: "+FlameworkVersion,
			"Previous Flamework Version: "+previous,
		)
	}
	runtimeConfig, present, err := LoadRuntimeConfig(projectRoot.packageDirectory)
	if err != nil {
		if messages := runtimeConfigSchemaDiagnostics(err); len(messages) > 0 {
			return nil, newProjectDiagnosticError(messages...)
		}
		return nil, err
	}
	buildInfo.SetRuntimeConfig(nil)
	var loadedRuntimeConfig *RuntimeConfig
	if present {
		loadedRuntimeConfig = &runtimeConfig
		buildInfo.SetRuntimeConfig(loadedRuntimeConfig)
	}
	if hashPrefix == "" {
		buildInfo.SetIdentifierPrefix(nil)
	} else {
		buildInfo.SetIdentifierPrefix(&hashPrefix)
	}
	return &Project{
		projectDirectory: projectDirectory,
		rootDirectory:    projectRoot.packageDirectory,
		outDirectory:     outDirectory,
		includeDirectory: includeDirectory,
		packageName:      projectRoot.packageName,
		packageVersion:   projectRoot.packageVersion,
		hashPrefix:       hashPrefix,
		idMode:           mode,
		isGame:           isGame,
		packMode:         packMode,
		config:           options.Config,
		translator:       translator,
		resolver:         resolver,
		analysis:         NewAnalysisState(),
		buildInfo:        buildInfo,
		runtimeConfig:    cloneRuntimeConfig(loadedRuntimeConfig),
	}, nil
}

func runtimeConfigSchemaDiagnostics(err error) []string {
	message := err.Error()
	for _, field := range []string{"profiling", "disableDependencyWarnings"} {
		if strings.Contains(message, field+" must be boolean") {
			return []string{"Malformed flamework.json", "type /" + field + `: must be boolean {"type":"boolean"}`}
		}
	}
	if strings.Contains(message, "logLevel must be none or verbose") {
		return []string{"Malformed flamework.json", `enum /logLevel: must be equal to one of the allowed values {"allowedValues":["none","verbose"]}`}
	}
	return nil
}

func newProjectDiagnosticError(messages ...string) error {
	items := make([]*ast.Diagnostic, len(messages))
	for index, message := range messages {
		items[index] = ast.NewDiagnosticWithStringCode(nil, core.UndefinedTextRange(), strings.TrimSpace(flameworkDiagnosticCode), diagnostics.CategoryError, message)
	}
	return &ProjectDiagnosticError{diagnostics: items}
}

func (p *Project) PackageName() string {
	return p.packageName
}

func (p *Project) PackageVersion() string {
	return p.packageVersion
}

func (p *Project) RootDirectory() string {
	return p.rootDirectory
}

func (p *Project) IncludeDirectory() string {
	return p.includeDirectory
}

func (p *Project) IsGame() bool {
	return p.isGame
}

func (p *Project) HashPrefix() string {
	return p.hashPrefix
}

func (p *Project) IDMode() IDMode {
	return p.idMode
}

func (p *Project) PathTranslator() *rojo.PathTranslator {
	return p.translator
}

func (p *Project) RojoResolver() *rojo.RojoResolver {
	return p.resolver
}

func (p *Project) Analyze(files []FileAnalysis) ([]FilePlan, error) {
	return p.analysis.Analyze(files)
}

func (p *Project) Plans() []FilePlan {
	return p.analysis.Plans()
}

func (m IDMode) valid() bool {
	switch m {
	case IDModeFull, IDModeObfuscated, IDModeShort, IDModeTiny:
		return true
	default:
		return false
	}
}
