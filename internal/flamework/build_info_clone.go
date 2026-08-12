package flamework

func cloneRuntimeConfig(config *RuntimeConfig) *RuntimeConfig {
	if config == nil {
		return nil
	}
	data, err := config.MarshalJSON()
	if err != nil {
		return nil
	}
	cloned, err := parseRuntimeConfig(data)
	if err != nil {
		return nil
	}
	return &cloned
}

func cloneMetadata(metadata *BuildMetadata) *BuildMetadata {
	if metadata == nil {
		return nil
	}
	cloned := &BuildMetadata{Config: cloneRuntimeConfig(metadata.Config)}
	if metadata.Globs != nil {
		cloned.Globs = &BuildGlobs{Paths: cloneStringSlicesMap(metadata.Globs.Paths), Origins: cloneStringSlicesMap(metadata.Globs.Origins)}
	}
	return cloned
}

func cloneStringSlicesMap(source *map[string][]string) *map[string][]string {
	if source == nil {
		return nil
	}
	cloned := make(map[string][]string, len(*source))
	for key, values := range *source {
		cloned[key] = append([]string(nil), values...)
	}
	return &cloned
}

func cloneStringPointer(value *string) *string {
	if value == nil {
		return nil
	}
	cloned := *value
	return &cloned
}

func cloneBuildClass(class BuildClass) BuildClass {
	class.Decorators = append([]BuildDecorator(nil), class.Decorators...)
	return class
}
