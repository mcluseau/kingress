package config

import (
	"net"
	"sort"
	"strings"
)

const (
	fromNginx = "From [ingress-nginx](https://github.com/kubernetes/ingress-nginx/blob/master/docs/user-guide/nginx-configuration/annotations.md"
)

var Annotations = []Annotation{
	// Handle some of the nginx ingress controller options
	// (see https://github.com/kubernetes/ingress-nginx/blob/master/docs/user-guide/annotations.md)
	{
		Name:        "ssl-redirect",
		Description: fromNginx + "#server-side-https-enforcement-through-redirect)",
		get:         func(o *BackendOptions) interface{} { return o.SSLRedirect },
		apply: func(o *BackendOptions, value string) error {
			o.SSLRedirect = boolOpt(value)
			return nil
		},
	},
	{
		Name:        "secure-backends",
		Description: "Make TLS connections to the upstream instead of plain HTTP. Initialy from ingress-nginx but removed from it, we still support it.",
		get:         func(o *BackendOptions) interface{} { return o.SecureBackends },
		apply: func(o *BackendOptions, value string) error {
			o.SecureBackends = boolOpt(value)
			return nil
		},
	},
	{
		Name:        "whitelist-source-range",
		Description: fromNginx + "#whitelist-source-range)",
		get:         func(o *BackendOptions) interface{} { return o.WhitelistSourceRange },
		apply: func(o *BackendOptions, value string) (err error) {
			o.WhitelistSourceRange, err = ipNetArray(value)
			return
		},
	},
	{
		Name:        "cors-allowed-origins",
		Description: "",
		get:         func(o *BackendOptions) any { return o.CORSAllowedOrigins },
		apply: func(o *BackendOptions, value string) (err error) {
			value = strings.TrimSpace(value)
			if len(value) == 0 {
				return
			}
			origins := strings.Split(value, ",")
			for i, o := range origins {
				origins[i] = strings.TrimSpace(o)
			}
			o.CORSAllowedOrigins = origins
			return
		},
	},
}

type Annotation struct {
	Name        string
	Description string
	apply       func(o *BackendOptions, value string) error
	get         func(o *BackendOptions) interface{}
}

type byName []Annotation

func (s byName) Len() int           { return len(s) }
func (s byName) Swap(i, j int)      { s[i], s[j] = s[j], s[i] }
func (s byName) Less(i, j int) bool { return s[i].Name < s[j].Name }

func init() {
	sort.Sort(byName(Annotations))
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
