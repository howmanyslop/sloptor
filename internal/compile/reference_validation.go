package compile

import (
	"fmt"
	"path/filepath"
	"slices"
	"strings"

	"rotor/internal/rojo"
	"rotor/tsgo/ast"
	"rotor/tsgo/diagnostics"
	"rotor/tsgo/outputpaths"
	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs/osvfs"
)

type referenceDiagnosticError struct {
	diagnostic DiagnosticInfo
}

func (e *referenceDiagnosticError) Error() string { return e.diagnostic.Message }

func validateSolutionReferences(graph *SolutionGraph, programs map[string]projectOutputHost) error {
	references := [][]string{}
	for _, project := range graph.Projects {
		references = append(references, project.References)
	}
	for _, chain := range graph.coordinatorConfigChains {
		references = append(references, chain.references)
	}
	for _, dependencies := range references {
		for _, configPath := range dependencies {
			program := programs[configPath]
			if program == nil || program.Options().OutDir == "" {
				continue
			}
			options := program.Options()
			var diagnostic *ast.Diagnostic
			if !options.Composite.IsTrue() {
				diagnostic = ast.NewCompilerDiagnostic(diagnostics.Referenced_project_0_must_have_setting_composite_Colon_true, configPath)
			} else if options.NoEmit.IsTrue() || options.Declaration.IsFalse() {
				diagnostic = ast.NewCompilerDiagnostic(diagnostics.Referenced_project_0_may_not_disable_emit, configPath)
			}
			if diagnostic != nil {
				return &referenceDiagnosticError{diagnostic: infoFromTSDiag(diagnostic, nil)}
			}
		}
	}
	return nil
}

func validateProjectOutputs(graph *SolutionGraph, programs map[string]projectOutputHost) error {
	type outputOwner struct{ path, configPath string }
	owners := []outputOwner{}
	buildInfoOwners := map[string]string{}
	sources := []string{}
	for _, project := range graph.Projects {
		if program := programs[project.ConfigPath]; program != nil {
			sources = append(sources, projectRootDirs(program)...)
		}
	}
	for _, project := range graph.Projects {
		program := programs[project.ConfigPath]
		if program == nil || program.Options().OutDir == "" {
			continue
		}
		translator := createPathTranslator(program, !project.Options.LuaExtension)
		roots := []string{translator.OutDir}
		options := program.Options()
		if options.GetEmitDeclarations() && options.DeclarationDir != "" {
			roots = append(roots, filepath.FromSlash(options.DeclarationDir))
		}
		for _, root := range roots {
			for _, source := range sources {
				if isPathDescendantOf(filepath.FromSlash(source), root) {
					return fmt.Errorf(`Output directory "%s" contains source directory "%s".`, root, filepath.FromSlash(source))
				}
			}
			for _, owner := range owners {
				if owner.configPath != project.ConfigPath && (isPathDescendantOf(root, owner.path) || isPathDescendantOf(owner.path, root)) {
					return fmt.Errorf(`Projects "%s" and "%s" have overlapping output directories. Use separate outDir and declarationDir paths.`, owner.configPath, project.ConfigPath)
				}
			}
			owners = append(owners, outputOwner{root, project.ConfigPath})
		}
		tsBuildInfoPath := outputpaths.GetBuildInfoFileName(options, tspath.ComparePathsOptions{CurrentDirectory: filepath.Dir(project.ConfigPath), UseCaseSensitiveFileNames: osvfs.FS().UseCaseSensitiveFileNames()})
		for _, path := range []string{translator.BuildInfoOutputPath, filepath.FromSlash(tsBuildInfoPath)} {
			if path == "" {
				continue
			}
			key := rojo.CanonicalFileName(path, osvfs.FS().UseCaseSensitiveFileNames())
			if _, exists := buildInfoOwners[key]; exists {
				return fmt.Errorf(`Multiple projects write "%s". Set a unique tsBuildInfoFile for each project.`, path)
			}
			buildInfoOwners[key] = project.ConfigPath
		}
	}
	for path, configPath := range buildInfoOwners {
		for _, owner := range owners {
			rootKey := rojo.CanonicalFileName(owner.path, osvfs.FS().UseCaseSensitiveFileNames())
			if owner.configPath != configPath && isPathDescendantOf(filepath.FromSlash(path), filepath.FromSlash(rootKey)) {
				return fmt.Errorf(`Build info "%s" is inside another project's output directory "%s".`, path, owner.path)
			}
		}
	}
	return nil
}

func validateReferenceRojoPaths(graph *SolutionGraph, outputs map[string][]string) error {
	return validateProjectsReferenceRojoPaths(graph, graph.Projects, outputs)
}

// validateProjectsReferenceRojoPaths checks the direct references of consumers
// only, so one project's mounts never fail an unrelated project.
func validateProjectsReferenceRojoPaths(graph *SolutionGraph, consumers []SolutionProject, outputs map[string][]string) error {
	needed := map[string]bool{}
	for _, project := range consumers {
		needed[project.ConfigPath] = true
		for _, configPath := range project.directReferences {
			needed[configPath] = true
		}
	}
	resolvers := map[string]*rojo.RojoResolver{}
	resolverPaths := map[string]string{}
	byRojoPath := map[string]*rojo.RojoResolver{}
	for _, project := range graph.Projects {
		if !needed[project.ConfigPath] {
			continue
		}
		path, _, err := resolveRojoConfigPath(filepath.Dir(project.ConfigPath), project.Options.RojoConfigPath)
		if err != nil {
			return err
		}
		resolverPaths[project.ConfigPath] = path
		if path == "" {
			continue
		}
		key := rojo.CanonicalFileName(path, osvfs.FS().UseCaseSensitiveFileNames())
		resolver := byRojoPath[key]
		if resolver == nil {
			resolver = rojo.FromPath(path)
			byRojoPath[key] = resolver
		}
		resolvers[project.ConfigPath] = resolver
	}
	// Upstream checks only direct references; a solution config has no path
	// translator, so projects reached through one are skipped.
	for _, project := range consumers {
		for _, configPath := range project.directReferences {
			paths, emits := outputs[configPath]
			owner, consumer := resolvers[configPath], resolvers[project.ConfigPath]
			declared, err := ReadRbxtsOptions(configPath)
			if err != nil {
				return err
			}
			if !emits || owner == nil || owner == consumer || declared == nil || declared.Rojo == nil || *declared.Rojo == "" {
				continue
			}
			for _, path := range paths {
				ownerPath, ownerMapped := owner.GetRbxPathFromFilePath(path)
				var consumerPath rojo.RbxPath
				consumerMapped := false
				if consumer != nil {
					consumerPath, consumerMapped = consumer.GetRbxPathFromFilePath(path)
				}
				if ownerMapped == consumerMapped && slices.Equal(ownerPath, consumerPath) {
					continue
				}
				mount := "unmapped"
				if ownerMapped {
					mount = strings.Join(ownerPath, ".")
				}
				return fmt.Errorf(`Project "%s" must mount "%s" at the same Roblox path as "%s" (%s). Referenced projects with their own rbxts.rojo require consistent module and runtime mounts.`, project.ConfigPath, path, resolverPaths[configPath], mount)
			}
		}
	}
	return nil
}
