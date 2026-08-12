package flamework

import (
	"fmt"
	"path/filepath"
	"sort"
	"strings"
)

func addPackageBuildClass(project *Project, class ClassPlan, fileName string) error {
	output := project.PathTranslator().GetOutputPath(fileName)
	relative, err := filepath.Rel(project.RootDirectory(), output)
	if err != nil {
		return fmt.Errorf("resolve Flamework build class path: %w", err)
	}
	classID, err := project.Identifier(DeclarationIdentity{InternalID: class.InternalID, DeclarationName: classNameFromInternalID(class.InternalID), LuaFileName: luaFileName(fileName)})
	if err != nil {
		return err
	}
	decorators := make([]BuildDecorator, len(class.Decorators))
	for index, decorator := range class.Decorators {
		identity := DeclarationIdentity{InternalID: decorator.InternalID, DeclarationName: classNameFromInternalID(decorator.InternalID), LuaFileName: luaFileName(fileName)}
		identity.PackagePrefix, identity.IsPackage, err = packagePrefixForInternalID(project, decorator.InternalID)
		if err != nil {
			return err
		}
		decoratorID, identifierErr := project.Identifier(identity)
		if identifierErr != nil {
			return identifierErr
		}
		decorators[index] = BuildDecorator{Name: decorator.Name, InternalID: decoratorID}
	}
	return project.AddClass(BuildClass{FilePath: filepath.ToSlash(relative), InternalID: classID, Decorators: decorators})
}

func packagePrefixForInternalID(project *Project, internalID string) (string, bool, error) {
	separator := strings.IndexByte(internalID, ':')
	if separator <= 0 || internalID[:separator] == project.PackageName() {
		return "", false, nil
	}
	packageName := internalID[:separator]
	build, err := LoadBuildInfo(filepath.Join(project.projectDirectory, "node_modules", filepath.FromSlash(packageName), "flamework.build"), FlameworkVersion)
	if err != nil {
		return "", false, fmt.Errorf("load packaged Flamework identifier metadata: %w", err)
	}
	prefix, _ := build.Snapshot().IdentifierPrefix()
	return prefix, true, nil
}

func buildClassByInternalID(snapshot BuildInfoSnapshot, internalID string) (BuildClass, bool) {
	index := sort.Search(len(snapshot.Classes), func(index int) bool { return snapshot.Classes[index].InternalID >= internalID })
	if index < len(snapshot.Classes) && snapshot.Classes[index].InternalID == internalID {
		return snapshot.Classes[index], true
	}
	for _, class := range snapshot.Classes {
		if class.InternalID == internalID {
			return class, true
		}
	}
	return BuildClass{}, false
}
