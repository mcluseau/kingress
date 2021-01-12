package k8s

import (
	"bytes"
	"context"
	"encoding/json"
	"log"
	"net"
	"strings"
	"time"

	v1 "k8s.io/api/core/v1"
	netv1 "k8s.io/api/networking/v1beta1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/util/intstr"
	"k8s.io/client-go/kubernetes"

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

type ingressHandler struct {
	k8s   *kubernetes.Clientset
	Hosts []string
}

func (h ingressHandler) OnAdd(obj interface{}) {
	h.update(obj.(*netv1.Ingress))
}

func (h ingressHandler) OnUpdate(oldObj, newObj interface{}) {
	h.update(newObj.(*netv1.Ingress))
}

func (h ingressHandler) OnDelete(obj interface{}) {
	h.delete(obj.(*netv1.Ingress))
}

func (h ingressHandler) update(ing *netv1.Ingress) {
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
		if tls.SecretName == "" {
			continue
		}

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

	// also check & update the status as needed
	lb := v1.LoadBalancerStatus{}

	for _, host := range h.Hosts {
		lbi := v1.LoadBalancerIngress{}

		if net.ParseIP(host) != nil {
			lbi.IP = host
		} else {
			lbi.Hostname = host
		}

		lb.Ingress = append(lb.Ingress, lbi)
	}

	curBytes, _ := json.Marshal(ing.Status.LoadBalancer.Ingress)
	newBytes, _ := json.Marshal(lb.Ingress)
	if !bytes.Equal(curBytes, newBytes) {
		log.Print("updating ingress status: ", ing.Namespace, "/", ing.Name, ": ", string(newBytes))
		ingClient := h.k8s.NetworkingV1beta1().Ingresses(ing.Namespace)

		ing.Status.LoadBalancer.Ingress = lb.Ingress

		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()

		_, err := ingClient.UpdateStatus(ctx, ing, metav1.UpdateOptions{})
		if err != nil {
			log.Print("failed to update ingress status: ", ing.Namespace, "/", ing.Name, ": ", err)
		}
	}
}

func (_ ingressHandler) delete(ing *netv1.Ingress) {
	ref := k8sRef(ing)

	config.Lock()
	defer config.Unlock()

	delete(ingressRules, ref)
	delete(ingressSecrets, ref)

	config.NotifyChanged(newConfig)
}
