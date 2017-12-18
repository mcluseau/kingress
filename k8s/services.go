package k8s

import (
	core "k8s.io/api/core/v1"
	"k8s.io/apimachinery/pkg/util/intstr"

	"github.com/MikaelCluseau/kingress/config"
)

var (
	services = map[string][]servicePort{}
)

type servicePort struct {
	Name       string
	Port       int32
	TargetPort intstr.IntOrString
}

type servicesHandler struct{}

func (h servicesHandler) OnAdd(obj interface{}) {
	h.update(obj.(*core.Service))
}

func (h servicesHandler) OnUpdate(oldObj, newObj interface{}) {
	h.update(newObj.(*core.Service))
}

func (h servicesHandler) OnDelete(obj interface{}) {
	h.delete(obj.(*core.Service))
}

func (_ servicesHandler) update(svc *core.Service) {
	ports := make([]servicePort, 0)

	for _, port := range svc.Spec.Ports {
		ports = append(ports, servicePort{
			Name:       port.Name,
			Port:       port.Port,
			TargetPort: port.TargetPort,
		})
	}

	config.Lock()
	defer config.Unlock()

	services[k8sRef(svc)] = ports

	config.NotifyChanged(newConfig)
}

func (_ servicesHandler) delete(svc *core.Service) {
	config.Lock()
	defer config.Unlock()

	delete(services, k8sRef(svc))

	config.NotifyChanged(newConfig)
}

func findTargetPort(svcRef string, port intstr.IntOrString) (intstr.IntOrString, bool) {
	ports := services[svcRef]

	switch port.Type {
	case intstr.Int:
		for _, svcPort := range ports {
			if svcPort.Port == port.IntVal {
				return svcPort.TargetPort, true
			}
		}

	case intstr.String:
		for _, svcPort := range ports {
			if svcPort.Name == port.StrVal {
				return svcPort.TargetPort, true
			}
		}

	}

	return intstr.FromString(""), false
}
