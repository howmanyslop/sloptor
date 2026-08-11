package config

type Config struct {
	Assets    *AssetsConfig    `json:"assets,omitempty" toml:"assets,omitempty"`
	Deploy    *DeployConfig    `json:"deploy,omitempty" toml:"deploy,omitempty"`
	Flamework *FlameworkConfig `json:"flamework,omitempty" toml:"flamework,omitempty"`
	Warnings  []string         `json:"-" toml:"-"`
}
