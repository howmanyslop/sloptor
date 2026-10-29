package config

type Config struct {
	ParallelTransformers *ParallelTransformersConfig `json:"parallelTransformers,omitempty" toml:"parallelTransformers,omitempty"`
	Flamework            *FlameworkConfig            `json:"flamework,omitempty" toml:"flamework,omitempty"`

	// NativeTypes is the rotor.toml half of the `//!native` type-annotation
	// opt-in: when true, a file whose leading comments include `//!native`
	// emits Luau parameter and return annotations. It is a pointer so an unset
	// value is distinguishable from an explicit false — tsconfig.json's
	// `rbxts.nativeTypes` wins when it is set, and this value is the fallback
	// only. Both unset means off.
	NativeTypes *bool `json:"nativeTypes,omitempty" toml:"nativeTypes,omitempty"`

	Warnings []string `json:"-" toml:"-"`
}
