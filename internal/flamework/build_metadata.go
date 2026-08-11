package flamework

func (b *BuildInfo) SetRuntimeConfig(config *RuntimeConfig) {
	if b.metadata == nil {
		b.metadata = &BuildMetadata{}
		b.ensureField("metadata")
	}
	b.metadata.Config = cloneRuntimeConfig(config)
}

func (b *BuildInfo) AddGlob(glob, origin string) {
	if b.metadata == nil {
		b.metadata = &BuildMetadata{}
		b.ensureField("metadata")
	}
	if b.metadata.Globs == nil {
		b.metadata.Globs = &BuildGlobs{}
	}
	if b.metadata.Globs.Paths == nil {
		paths := map[string][]string{}
		b.metadata.Globs.Paths = &paths
	}
	if b.metadata.Globs.Origins == nil {
		origins := map[string][]string{}
		b.metadata.Globs.Origins = &origins
	}
	(*b.metadata.Globs.Paths)[glob] = []string{}
	(*b.metadata.Globs.Origins)[origin] = append((*b.metadata.Globs.Origins)[origin], glob)
}

func (b *BuildInfo) SetGlobPaths(paths map[string][]string) {
	if b.metadata == nil {
		b.metadata = &BuildMetadata{}
		b.ensureField("metadata")
	}
	if b.metadata.Globs == nil {
		b.metadata.Globs = &BuildGlobs{}
	}
	cloned := make(map[string][]string, len(paths))
	for pattern, matches := range paths {
		cloned[pattern] = append([]string(nil), matches...)
	}
	b.metadata.Globs.Paths = &cloned
}

func (b *BuildInfo) InvalidateGlobs(origin string) {
	if b.metadata == nil || b.metadata.Globs == nil || b.metadata.Globs.Paths == nil || b.metadata.Globs.Origins == nil {
		return
	}
	delete(*b.metadata.Globs.Origins, origin)
	for glob := range *b.metadata.Globs.Paths {
		used := false
		for _, globs := range *b.metadata.Globs.Origins {
			for _, candidate := range globs {
				used = used || candidate == glob
			}
		}
		if !used {
			delete(*b.metadata.Globs.Paths, glob)
		}
	}
}
