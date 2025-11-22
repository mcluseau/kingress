package k8s

import (
	"fmt"

	core "k8s.io/api/core/v1"

	"github.com/mcluseau/kingress/config"
)

var (
	endpoints = map[string][]endpointSpec{}
)

type endpointSpec struct {
	Name   string
	Target string
}

type endpointsHandler struct{}

func (h endpointsHandler) OnAdd(obj interface{}, isInInitialList bool) {
	h.update(obj.(*core.Endpoints))
}

func (h endpointsHandler) OnUpdate(oldObj, newObj interface{}) {
	h.update(newObj.(*core.Endpoints))
}

func (h endpointsHandler) OnDelete(obj interface{}) {
	h.delete(obj.(*core.Endpoints))
}

func (_ endpointsHandler) update(ep *core.Endpoints) {
	eps := make([]endpointSpec, 0)

	for _, subset := range ep.Subsets {
		for _, addr := range subset.Addresses {
			for _, port := range subset.Ports {
				eps = append(eps, endpointSpec{
					Name:   port.Name,
					Target: fmt.Sprintf("%s:%d", addr.IP, port.Port),
				})
			}
		}
	}

	config.Lock()
	defer config.Unlock()

	endpoints[k8sRef(ep)] = eps

	config.NotifyChanged(newConfig)
}

func (_ endpointsHandler) delete(ep *core.Endpoints) {
	config.Lock()
	defer config.Unlock()

	delete(endpoints, k8sRef(ep))

	config.NotifyChanged(newConfig)
}

func findEndpoints(svcRef string, portName string) []string {
	targets := make([]string, 0)

	eps := endpoints[svcRef]

	for _, ep := range eps {
		if ep.Name == portName {
			targets = append(targets, ep.Target)
		}
	}

	return targets
}
