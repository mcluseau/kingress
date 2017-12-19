package k8s

import (
	"github.com/MikaelCluseau/kingress/config"
)

type backendsOrder []*config.Backend

func (a backendsOrder) Len() int           { return len(a) }
func (a backendsOrder) Swap(i, j int)      { a[i], a[j] = a[j], a[i] }
func (a backendsOrder) Less(i, j int) bool { return len(a[i].Prefix()) > len(a[j].Prefix()) }
