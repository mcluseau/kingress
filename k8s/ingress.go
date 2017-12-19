package k8s

import (
	"log"
	"strings"

	ext "k8s.io/api/extensions/v1beta1"
	"k8s.io/apimachinery/pkg/util/intstr"

	"github.com/mcluseau/kingress/config"
)

var (
	ingressRules   = map[string][]ingressRule{}
	ingressSecrets = map[string][]ingressTLS{}
)

type ingressRule struct {
	Host    string
	Path    string
	Service string
	Port    intstr.IntOrString
	Options *config.BackendOptions
}

type ingressTLS struct {
	Host      string
	SecretRef string
}

type ingressHandler struct{}

func (h ingressHandler) OnAdd(obj interface{}) {
	h.update(obj.(*ext.Ingress))
}

func (h ingressHandler) OnUpdate(oldObj, newObj interface{}) {
	h.update(newObj.(*ext.Ingress))
}

func (h ingressHandler) OnDelete(obj interface{}) {
	h.delete(obj.(*ext.Ingress))
}

func (_ ingressHandler) update(ing *ext.Ingress) {
	ref := k8sRef(ing)

	// parse ingress options
	opts := &config.BackendOptions{}
	for key, value := range ing.Annotations {
		keyParts := strings.SplitN(key, "/", 2)

		if len(keyParts) != 2 {
			continue
		}

		shouldBeKnown := false
		switch keyParts[0] {
		case "kubernetes.io":
		// ok

		case "ingress.kubernetes.io", "nginx.ingress.kubernetes.io":
			shouldBeKnown = true

		default:
			continue
		}

		known, err := opts.Set(keyParts[1], value)
		if err != nil {
			log.Printf("warning: ingress %s: error parsing annotation %s: %s", ref, key, err)

		} else if shouldBeKnown && !known {
			log.Printf("warning: ingress %s: unknown annotation: %s", ref, key)
		}
	}

	rules := make([]ingressRule, 0)

	// Collect host,path->target associations
	for _, rule := range ing.Spec.Rules {
		for _, path := range rule.HTTP.Paths {
			rules = append(rules, ingressRule{
				Host:    rule.Host,
				Path:    path.Path,
				Service: ing.Namespace + "/" + path.Backend.ServiceName,
				Port:    path.Backend.ServicePort,
				Options: opts,
			})
		}
	}

	// Collect host->secret associations
	secrets := make([]ingressTLS, 0)
	for _, tls := range ing.Spec.TLS {
		secretRef := ing.Namespace + "/" + tls.SecretName

		for _, host := range tls.Hosts {
			secrets = append(secrets, ingressTLS{
				Host:      host,
				SecretRef: secretRef,
			})
		}
	}

	config.Lock()
	defer config.Unlock()

	ingressRules[ref] = rules
	ingressSecrets[ref] = secrets

	config.NotifyChanged(newConfig)
}

func (_ ingressHandler) delete(ing *ext.Ingress) {
	ref := k8sRef(ing)

	config.Lock()
	defer config.Unlock()

	delete(ingressRules, ref)
	delete(ingressSecrets, ref)

	config.NotifyChanged(newConfig)
}
