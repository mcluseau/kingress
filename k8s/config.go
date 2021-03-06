package k8s

import (
	"crypto/tls"
	"fmt"
	"sort"

	"github.com/mcluseau/kingress/config"
)

func newConfig() config.Config {
	newBackends := config.Backends{}
	errors := make([]string, 0)

	for ingRef, rules := range ingressRules {
	rulesLoop:
		for _, rule := range rules {
			backends := newBackends[rule.Host]

			for _, backend := range backends {
				if backend.Prefix == rule.Path {
					errors = append(errors, fmt.Sprintf(
						"warning: duplicate definition for host %s, path %v: ignoring ingress %s rule to %s:%s",
						rule.Host, rule.Path, ingRef, rule.Service, rule.Port.String()))
					continue rulesLoop
				}
			}

			// lookup the target port for the ingress' target service/port
			targetPort, ok := findTargetPort(rule.Service, rule.Port)
			if !ok {
				// we may log here but this kind of problem is visible in the endpoints
				// log.Printf("no target for service %s, port %s (ingress: %s)", rule.Service, rule.Port.String(), ingRef)
				continue
			}

			// build the backend from the service endpoints
			backend := config.NewBackend(ingRef, rule.Path, findEndpoints(rule.Service, targetPort)...)

			if rule.Options != nil {
				backend.Options = *rule.Options
			}

			newBackends[rule.Host] = append(backends, backend)
		}
	}

	// Sort each host's backends by reverse length
	for _, backends := range newBackends {
		sort.Sort(backendsOrder(backends))
	}

	newCerts := map[string]*tls.Certificate{}
	for ingRef, ingTLSs := range ingressSecrets {
		for _, ingTLS := range ingTLSs {
			cert, ok := secretCertificate[ingTLS.SecretRef]
			if !ok {
				errors = append(errors, fmt.Sprintf(
					"warning: no TLS secret %s for host %s (ingress: %s)",
					ingTLS.SecretRef, ingTLS.Host, ingRef))
				continue
			}

			newCerts[ingTLS.Host] = cert
		}
	}

	sort.Strings(errors)

	return config.Config{
		Errors:       errors,
		HostBackends: newBackends,
		HostCerts:    newCerts,
		DefaultCert:  defaultCert,
	}
}
