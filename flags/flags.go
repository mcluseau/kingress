package flags

import "flag"

var Verbose = false

func init() {
	flag.BoolVar(&Verbose, "v", Verbose, "verbose logging")
}
