package config

// ParallelTransformersConfig is an experimental, project-owned attestation.
// Unsupported or incomplete attestations select the ordinary serial path.
// Config is the complete plugin options object encoded as JSON, including
// transform. The order must equal the entire initial external stage.
type ParallelTransformersConfig struct {
	Workers          int                             `json:"workers" toml:"workers"`
	BatchIndependent bool                            `json:"batchIndependent" toml:"batchIndependent"`
	TypeScript       string                          `json:"typescript" toml:"typescript"`
	Plugins          []ParallelTransformerPlugin     `json:"plugins" toml:"plugins"`
	Dependencies     []ParallelTransformerDependency `json:"dependencies" toml:"dependencies"`
}

type ParallelTransformerPlugin struct {
	Config string `json:"config" toml:"config"`
	SHA256 string `json:"sha256" toml:"sha256"`
}

// From selects the plugin whose normal Node resolution path owns this
// patched/local dependency. The owner lists every such load-path dependency.
type ParallelTransformerDependency struct {
	From      int    `json:"from" toml:"from"`
	Specifier string `json:"specifier" toml:"specifier"`
	SHA256    string `json:"sha256" toml:"sha256"`
}
