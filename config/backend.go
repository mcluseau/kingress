package config

import (
	"math/rand"
	"strings"
)

type Backend struct {
	IngressRef string
	Prefix     string
	Targets    []string

	Options BackendOptions
}

func NewBackend(ingressRef, prefix string, targets ...string) *Backend {
	return &Backend{
		IngressRef: ingressRef,
		Prefix:     prefix,
		Targets:    targets,
	}
}

func (b *Backend) HandlesPath(path string) bool {
	return strings.HasPrefix(path, b.Prefix)
}

func (b *Backend) Target() string {
	if len(b.Targets) == 0 {
		return ""
	}

	target := b.Targets[rand.Intn(len(b.Targets))]

	return target
}
