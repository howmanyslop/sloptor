package flamework

import (
	"fmt"
	"testing"
)

var buildLookupBenchmarkDigest uint64

// BenchmarkRootBuildClassLookup measures lookup plus complete result consumption,
// not compiler latency. Run only under a separate measurement lease.
func BenchmarkRootBuildClassLookup(b *testing.B) {
	for _, metadata := range []int{0, 1024, 16384} {
		for _, decorators := range []int{0, 2} {
			for _, query := range []string{"c032", "mapped", "missing"} {
				b.Run(fmt.Sprintf("metadata=%d/decorators=%d/%s", metadata, decorators, query), func(b *testing.B) {
					info := NewBuildInfo("benchmark/flamework.build", FlameworkVersion)
					info.identifiers.set("mapped", "c032")
					hashes := newOrderedStringMap()
					info.stringHashes = &hashes
					for index := 0; index < metadata; index++ {
						key := fmt.Sprintf("unrelated-%05d", index)
						info.identifiers.set(key, key+"-id")
						hashes.set(key, key+"-hash")
					}
					classes := make([]BuildClass, 64)
					for index := range classes {
						classes[index] = BuildClass{InternalID: fmt.Sprintf("c%03d", index), FilePath: fmt.Sprintf("out/c%03d.lua", index)}
					}
					if decorators != 0 {
						classes[32].Decorators = []BuildDecorator{{Name: "Service", InternalID: "service"}, {Name: "Reflect", InternalID: "reflect"}}
					}
					info.classes = &classes
					project := &Project{buildInfo: info}
					var digest uint64
					consume := func(value string) {
						for index := range value {
							digest = digest*33 + uint64(value[index])
						}
					}
					b.ReportAllocs()
					// B.Loop excludes the fixture setup and post-loop sink publication.
					for b.Loop() {
						class, found := rootBuildClassByInternalID(project, query)
						consume(class.InternalID)
						consume(class.FilePath)
						for _, decorator := range class.Decorators {
							consume(decorator.Name)
							consume(decorator.InternalID)
						}
						if found {
							digest++
						}
					}
					buildLookupBenchmarkDigest = digest
				})
			}
		}
	}
}
