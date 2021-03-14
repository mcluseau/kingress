package k8s

import (
	"flag"
	"log"
	"sync"
	"time"

	"github.com/mcluseau/kingress/kubeclient"
	corev1 "k8s.io/api/core/v1"
	netv1 "k8s.io/api/networking/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/fields"
	"k8s.io/apimachinery/pkg/runtime"
	"k8s.io/client-go/tools/cache"
)

var (
	listOpts = metav1.ListOptions{}
	stopCh   chan struct{}
	wg       = &sync.WaitGroup{}

	namespace    = flag.String("namespace", metav1.NamespaceAll, "Namespace (defaults to all)")
	selector     = flag.String("selector", "", "Ingress selector")
	resyncPeriod = flag.Duration("resync-period", 10*time.Minute, "Period between full resyncs with Kubernetes")
)

func Start(hosts []string) {
	stopCh = make(chan struct{}, 1)

	c := kubeclient.Client()

	// watch ingresses
	watchK8s(c.NetworkingV1().RESTClient(), "ingresses", *selector, &netv1.Ingress{}, ingressHandler{c, hosts})

	// watch services & endpoints
	watchK8s(c.CoreV1().RESTClient(), "services", "", &corev1.Service{}, servicesHandler{})
	watchK8s(c.CoreV1().RESTClient(), "endpoints", "", &corev1.Endpoints{}, endpointsHandler{})

	// watch secrets
	watchK8s(c.CoreV1().RESTClient(), "secrets", "", &corev1.Secret{}, secretsHandler{})
}

func Stop() {
	close(stopCh)
	wg.Wait()
}

func watchK8s(c cache.Getter, resource, selector string, objType runtime.Object, h cache.ResourceEventHandler) {
	var sel fields.Selector

	if selector == "" {
		sel = fields.Everything()
	} else {
		sel = fields.ParseSelectorOrDie(selector)
	}

	lw := cache.NewListWatchFromClient(c, resource, *namespace, sel)

	_, ctr := cache.NewInformer(lw, objType, *resyncPeriod, h)

	wg.Add(1)
	go func() {
		defer wg.Done()

		log.Print("kubernetes: watching ", resource)
		ctr.Run(stopCh)
		log.Print("kubernetes: stopped watching ", resource)
	}()
}

// namespace/name
func k8sRef(obj metav1.Object) string {
	return obj.GetNamespace() + "/" + obj.GetName()
}
