package flamework

import (
	"fmt"
	"path/filepath"
	"sort"
)

const FlameworkVersion = "1.3.2"

type DeclarationIdentity struct {
	InternalID      string
	DeclarationName string
	LuaFileName     string
	PackagePrefix   string
	IsPackage       bool
}

func (p *Project) Identifier(declaration DeclarationIdentity) (string, error) {
	if existing, ok := p.buildInfo.Identifier(declaration.InternalID); ok {
		return existing, nil
	}
	salt := p.config.Salt
	if !declaration.IsPackage && p.idMode != IDModeFull && salt == "" {
		var err error
		salt, err = p.identifierSalt()
		if err != nil {
			return "", err
		}
	}
	id, err := GenerateIdentifier(IdentifierRequest{
		Mode:            p.idMode,
		Salt:            salt,
		HashPrefix:      p.hashPrefix,
		PackageName:     p.packageName,
		InternalID:      declaration.InternalID,
		DeclarationName: declaration.DeclarationName,
		LuaFileName:     declaration.LuaFileName,
		PackagePrefix:   declaration.PackagePrefix,
		NextID:          uint64(p.buildInfo.LatestID()),
		IsGame:          p.isGame,
		IsPackage:       declaration.IsPackage,
	})
	if err != nil {
		return "", fmt.Errorf("generate identifier %q: %w", declaration.InternalID, err)
	}
	if declaration.IsPackage {
		return id, nil
	}
	if err := p.buildInfo.AddIdentifier(declaration.InternalID, id); err != nil {
		return "", err
	}
	return id, nil
}

func (p *Project) PreloadIdentifiers(declarations []DeclarationIdentity) (map[string]string, error) {
	ordered := append([]DeclarationIdentity(nil), declarations...)
	sort.Slice(ordered, func(left, right int) bool {
		return ordered[left].InternalID < ordered[right].InternalID
	})
	identifiers := make(map[string]string, len(ordered))
	for _, declaration := range ordered {
		if _, exists := identifiers[declaration.InternalID]; exists {
			return nil, fmt.Errorf("%w: %s", ErrDuplicateIdentifier, declaration.InternalID)
		}
		identifier, err := p.Identifier(declaration)
		if err != nil {
			return nil, err
		}
		identifiers[declaration.InternalID] = identifier
	}
	return identifiers, nil
}

func (p *Project) HashString(text, context string) (string, error) {
	if context == "" {
		context = DefaultHashContext
	}
	return p.buildInfo.HashString(text, context)
}

func (p *Project) AddPackageBuildInfo(path string) error {
	packageInfo, err := LoadBuildInfo(path, FlameworkVersion)
	if err != nil {
		return fmt.Errorf("load package Flamework build info %q: %w", path, err)
	}
	return p.buildInfo.AddPackage(packageInfo)
}

func (p *Project) AddClass(class BuildClass) error {
	return p.buildInfo.AddClass(class)
}

func (p *Project) BuildInfoSnapshot() BuildInfoSnapshot {
	return p.buildInfo.Snapshot()
}

func (p *Project) PackageBuildInfoSnapshots() []BuildInfoSnapshot {
	return p.buildInfo.PackageSnapshots()
}

func (p *Project) PrepareArtifacts(configJSON, globsJSON []byte) ([]Artifact, error) {
	var artifacts []Artifact
	if !p.isGame {
		buildJSON, err := p.buildInfo.MarshalOrderedJSON()
		if err != nil {
			return nil, err
		}
		artifacts = []Artifact{{Path: p.buildInfo.Path(), Data: buildJSON}}
	} else {
		var err error
		artifacts, err = PrepareArtifacts(p.buildInfo, filepath.Join(p.includeDirectory, "flamework"), configJSON, globsJSON)
		if err != nil {
			return nil, err
		}
	}
	for index := range artifacts {
		relative, err := filepath.Rel(p.rootDirectory, artifacts[index].Path)
		if err != nil {
			return nil, fmt.Errorf("flamework: resolve artifact path %q: %w", artifacts[index].Path, err)
		}
		path, err := localArtifactPath(relative)
		if err != nil {
			return nil, err
		}
		artifacts[index].Path = path
	}
	return artifacts, nil
}

func (p *Project) PersistArtifacts(configJSON, globsJSON []byte) error {
	artifacts, err := p.PrepareArtifacts(configJSON, globsJSON)
	if err != nil {
		return err
	}
	return PersistArtifacts(p.rootDirectory, artifacts)
}

func (p *Project) identifierSalt() (string, error) {
	if p.config.Salt != "" {
		return p.config.Salt, nil
	}
	if salt, ok := p.buildInfo.Salt(); ok {
		return salt, nil
	}
	salt, err := NewSalt()
	if err != nil {
		return "", err
	}
	p.buildInfo.SetSalt(&salt)
	return salt, nil
}
