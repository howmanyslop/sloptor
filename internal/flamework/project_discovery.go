package flamework

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"rotor/internal/rojo"
)

var ErrInvalidPackage = errors.New("invalid Flamework package.json")

type projectRoot struct {
	directory        string
	packageDirectory string
	packageName      string
	packageVersion   string
}

func openProjectRoot(directory string) (projectRoot, error) {
	projectDirectory, err := filepath.Abs(directory)
	if err != nil {
		return projectRoot{}, fmt.Errorf("resolve Flamework project directory: %w", err)
	}
	packagePath := findPackageJSON(projectDirectory)
	if packagePath == "" {
		return projectRoot{}, upstreamBoundaryError("package.json not found in " + projectDirectory)
	}
	packageData, err := os.ReadFile(packagePath)
	if err != nil {
		return projectRoot{}, fmt.Errorf("read Flamework package identity: %w", err)
	}
	var identity struct {
		Name    string `json:"name"`
		Version string `json:"version"`
	}
	if err := json.Unmarshal(packageData, &identity); err != nil || identity.Name == "" {
		return projectRoot{}, fmt.Errorf("%w at %s", ErrInvalidPackage, packagePath)
	}
	return projectRoot{
		directory:        projectDirectory,
		packageDirectory: filepath.Dir(packagePath),
		packageName:      identity.Name,
		packageVersion:   identity.Version,
	}, nil
}

func resolveProjectPath(projectDirectory, configured, fallback string) (string, error) {
	value := configured
	if value == "" {
		value = filepath.Join(projectDirectory, fallback)
	} else if !filepath.IsAbs(value) {
		value = filepath.Join(projectDirectory, value)
	}
	resolved, err := filepath.Abs(value)
	if err != nil {
		return "", fmt.Errorf("resolve Flamework path %q: %w", value, err)
	}
	return filepath.Clean(resolved), nil
}

func openRojoResolver(projectDirectory, outDirectory, configured string) (*rojo.RojoResolver, error) {
	configPath := configured
	if configPath == "" {
		configPath, _ = rojo.FindRojoConfigFilePath(projectDirectory)
	} else if !filepath.IsAbs(configPath) {
		configPath = filepath.Join(projectDirectory, configPath)
	}
	if configPath == "" {
		return rojo.Synthetic(outDirectory), nil
	}
	if _, err := os.Stat(configPath); err != nil {
		return nil, fmt.Errorf("read Flamework Rojo configuration: %w", err)
	}
	return rojo.FromPath(configPath), nil
}

func findPackageJSON(directory string) string {
	for current := filepath.Clean(directory); ; current = filepath.Dir(current) {
		candidate := filepath.Join(current, "package.json")
		if info, err := os.Stat(candidate); err == nil && info.Mode().IsRegular() {
			return candidate
		}
		parent := filepath.Dir(current)
		if parent == current {
			return ""
		}
	}
}

func loadProjectBuildInfo(projectDirectory, packageDirectory string) (*BuildInfo, error) {
	path := filepath.Join(projectDirectory, "flamework.build")
	if _, err := os.Stat(path); errors.Is(err, os.ErrNotExist) && projectDirectory != packageDirectory {
		packagePath := filepath.Join(packageDirectory, "flamework.build")
		if _, packageErr := os.Stat(packagePath); packageErr == nil {
			path = packagePath
		}
	}
	info, err := LoadBuildInfo(path, FlameworkVersion)
	if err != nil {
		return nil, fmt.Errorf("load project Flamework build info: %w", err)
	}
	return info, nil
}
