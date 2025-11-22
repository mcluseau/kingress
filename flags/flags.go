package flags

import "flag"

var Verbose = false
var IngressClass = ""

func init() {
	flag.BoolVar(&Verbose, "v", Verbose, "verbose logging")
	flag.StringVar(&IngressClass, "ingress-class", "", "ingress class name to manage (all if unset)")
}
