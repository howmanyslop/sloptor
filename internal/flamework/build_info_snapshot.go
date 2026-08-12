package flamework

func (b *BuildInfo) Snapshot() BuildInfoSnapshot {
	identifiers := make(map[string]string, len(b.identifiers.values))
	for key, value := range b.identifiers.values {
		identifiers[key] = value
	}
	var classes []BuildClass
	if b.classes != nil {
		classes = make([]BuildClass, len(*b.classes))
		for index, class := range *b.classes {
			classes[index] = cloneBuildClass(class)
		}
	}
	return BuildInfoSnapshot{
		Path: b.path, Version: b.version, FlameworkVersion: b.flameworkVersion,
		Prefix: cloneStringPointer(b.prefix), Salt: cloneStringPointer(b.salt),
		Metadata: cloneMetadata(b.metadata), StringHashes: b.snapshotStringHashes(),
		Identifiers: identifiers, Classes: classes,
	}
}

func (b *BuildInfo) PackageSnapshots() []BuildInfoSnapshot {
	var snapshots []BuildInfoSnapshot
	for _, child := range b.packages {
		snapshots = append(snapshots, child.Snapshot())
		snapshots = append(snapshots, child.PackageSnapshots()...)
	}
	return snapshots
}

func (b *BuildInfo) snapshotStringHashes() map[string]string {
	if b.stringHashes == nil {
		return nil
	}
	result := make(map[string]string, len(b.stringHashes.values))
	for key, value := range b.stringHashes.values {
		result[key] = value
	}
	return result
}
