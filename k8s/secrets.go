package k8s

import (
	"crypto/tls"
	"flag"
	"log"

	core "k8s.io/api/core/v1"

	"github.com/mcluseau/kingress/config"
)

var (
	secretCertificate = map[string]*tls.Certificate{}
	defaultCert       *tls.Certificate

	tlsSecretName = flag.String("tls-secret", "default/kingress-default", "Default TLS secret (format: namespace/name)")
)

type secretsHandler struct{}

func (h secretsHandler) OnAdd(obj interface{}, isInInitialList bool) {
	h.update(obj.(*core.Secret))
}
func (h secretsHandler) OnUpdate(oldObj, newObj interface{}) {
	h.update(newObj.(*core.Secret))
}
func (h secretsHandler) OnDelete(obj interface{}) {
	h.delete(obj.(*core.Secret))
}

func (h secretsHandler) update(secret *core.Secret) {
	if secret.Type != core.SecretTypeTLS {
		h.delete(secret) // can secrets change type? I suppose not but better be safe
		return
	}

	ref := k8sRef(secret)

	cert, err := tls.X509KeyPair(secret.Data["tls.crt"], secret.Data["tls.key"])

	if err != nil {
		log.Printf("error: tls secret %s is invalid: %v", ref, err)
		h.delete(secret)
		return
	}

	config.Lock()
	defer config.Unlock()

	secretCertificate[ref] = &cert

	if ref == *tlsSecretName {
		defaultCert = &cert
	}

	config.NotifyChanged(newConfig)
}

func (_ secretsHandler) delete(secret *core.Secret) {
	config.Lock()
	defer config.Unlock()

	delete(secretCertificate, k8sRef(secret))

	config.NotifyChanged(newConfig)
}
