package config

import (
	"net"
	"strings"
)

type BackendOptions struct {
	SSLRedirect          bool
	SecureBackends       bool
	WhitelistSourceRange []*net.IPNet
}

func (o *BackendOptions) Set(key, value string) (known bool, err error) {
	known = true

	// Try to handle some of the nginx ingress controller options
	// (see https://github.com/kubernetes/ingress-nginx/blob/master/docs/user-guide/annotations.md)
	switch key {
	case "ssl-redirect":
		o.SSLRedirect = boolOpt(value)

	case "secure-backends":
		o.SecureBackends = boolOpt(value)

	case "whitelist-source-range":
		o.WhitelistSourceRange, err = ipNetArray(value)

	default:
		known = false
	}

	return
}

func boolOpt(value string) bool {
	return value == "true"
}

func ipNetArray(value string) ([]*net.IPNet, error) {
	if value == "" {
		return nil, nil
	}

	values := strings.Split(value, ",")
	nets := make([]*net.IPNet, len(values))

	for i, v := range values {
		_, ipnet, err := net.ParseCIDR(strings.TrimSpace(v))
		if err != nil {
			// on error, return an empty (fail safe to no allowed nets)
			return []*net.IPNet{}, err
		}

		nets[i] = ipnet
	}

	return nets, nil
}
