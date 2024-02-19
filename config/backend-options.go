package config

import (
	"net"
	"sort"
)

type BackendOptions struct {
	SSLRedirect          bool
	SecureBackends       bool
	WhitelistSourceRange []*net.IPNet
	CORSAllowedOrigins   []string
}

func (o *BackendOptions) Set(key, value string) (bool, error) {
	// search by name in sorted annotations set
	i := sort.Search(len(Annotations), func(i int) bool {
		return Annotations[i].Name >= key
	})

	if i >= len(Annotations) || Annotations[i].Name != key {
		return false, nil
	}

	err := Annotations[i].apply(o, value)

	return true, err
}

func (o *BackendOptions) Get() map[string]any {
	ret := make(map[string]any, len(Annotations))
	for _, ann := range Annotations {
		ret[ann.Name] = ann.get(o)
	}
	return ret
}
