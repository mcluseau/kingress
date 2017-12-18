package config

import (
	"strings"
)

type Backend struct {
	prefix  string
	targets []string
	n       int
}

func NewBackend(prefix string, targets ...string) *Backend {
	return &Backend{
		prefix:  prefix,
		targets: targets,
	}
}

func (b *Backend) Prefix() string {
	return b.prefix
}

func (b *Backend) HandlesPath(path string) bool {
	return strings.HasPrefix(path, b.prefix)
}

func (b *Backend) Target() string {
	if len(b.targets) == 0 {
		return ""
	}

	target := b.targets[b.n%len(b.targets)]
	b.n += 1

	return target
}
