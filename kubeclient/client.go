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

	masterURL  = flag.String("master", "", "The address of the Kubernetes API server")
	kubeconfig = flag.String("kubeconfig", "", "Path to a kubeconfig. Only required if out-of-cluster. Defaults to envvar KUBECONFIG.")
)

func Client() *kubernetes.Clientset {
	once.Do(connect)
	return k
}

func connect() {
	// Use in-cluster config or provide options
	var err error

	if *kubeconfig == "" {
		*kubeconfig = os.Getenv("KUBECONFIG")
	}

	cfg, err := clientcmd.BuildConfigFromFlags(*masterURL, *kubeconfig)
	if err != nil {
		log.Fatalf("Error building kubeconfig: %s", err.Error())
	}

	c, err := kubernetes.NewForConfig(cfg)
	if err != nil {
		log.Fatalf("Error building kubernetes client: %s", err.Error())
	}

	k = c
	log.Print("kubernetes: connected to ", cfg.Host)
}
