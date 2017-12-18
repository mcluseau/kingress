package kubeclient

import (
	"flag"
	"log"
	"os"
	"sync"

	"k8s.io/client-go/kubernetes"
	restclient "k8s.io/client-go/rest"
	"k8s.io/client-go/tools/clientcmd"
)

var (
	once   = sync.Once{}
	k      *kubernetes.Clientset
	config *restclient.Config

	hostFlag = flag.String("master", "", "The address of the Kubernetes API server")
)

func Client() *kubernetes.Clientset {
	once.Do(connect)
	return k
}

func connect() {
	// Use in-cluster config or provide options
	var err error
	config, err = restclient.InClusterConfig()
	if err != nil {
		// TODO check the Kubernetes project's standards (default flags somewhere?)
		config, err = clientcmd.BuildConfigFromFlags("", os.Getenv("HOME")+"/.kube/config")
		if err = restclient.SetKubernetesDefaults(config); err != nil {
			panic(err)
		}
	}

	if *hostFlag != "" {
		config.Host = *hostFlag
	}

	c, err := kubernetes.NewForConfig(config)
	if err != nil {
		panic(err)
	}
	k = c
	log.Print("kubernetes: connected to ", config.Host)
}
