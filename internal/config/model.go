package config

type Config struct {
	ParallelTransformers *ParallelTransformersConfig `json:"parallelTransformers,omitempty" toml:"parallelTransformers,omitempty"`
	Flamework            *FlameworkConfig            `json:"flamework,omitempty" toml:"flamework,omitempty"`
	Warnings             []string                    `json:"-" toml:"-"`
}
