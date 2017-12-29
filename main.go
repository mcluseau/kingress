package main

import (
	crand "crypto/rand"
	"flag"
	"log"
	"math"
	"math/big"
	"math/rand"
	"net/http"
	"os"
	"os/signal"
	"runtime/pprof"
	"syscall"

	"github.com/mcluseau/kingress/k8s"
)

var (
	httpBind     = flag.String("http", ":80", "HTTP bind specification (empty to disable)")
	httpsBind    = flag.String("https", ":443", "HTTPS bind specification (empty to disable)")
	sslRedirBind = flag.String("ssl-redirect", "", "HTTP to HTTPS redirector bind specification (empty to disable)")
	cpuProf      = flag.String("pprof-cpu", "", "Enable CPU profiling to this file")
)

func main() {
	flag.Set("logtostderr", "true")
	flag.Parse()

	if len(*cpuProf) != 0 {
		f, err := os.Create(*cpuProf)
		if err != nil {
			log.Fatal(err)
		}
		pprof.StartCPUProfile(f)
		defer pprof.StopCPUProfile()
	}

	go processLog()

	log.Print("Starting...")

	// seed math/rand
	{
		v, err := crand.Int(crand.Reader, big.NewInt(math.MaxInt64))
		if err != nil {
			log.Fatal("failed to read a random value: ", err)
		}
		rand.Seed(v.Int64())
	}

	// Start watching kubernetes
	k8s.Start()

	// HTTP
	if len(*httpBind) != 0 {
		go func() {
			err := startHTTP(*httpBind)
			log.Fatal("http handler finished: ", err)
		}()
	}

	// HTTPS
	if len(*httpsBind) != 0 {
		go func() {
			err := startHTTPS(*httpsBind)
			log.Fatal("https handler finished: ", err)
		}()
	}

	// HTTP to HTTPS
	if len(*sslRedirBind) != 0 {
		go func() {
			log.Print("ssl-redirect: listening on ", *sslRedirBind)
			err := http.ListenAndServe(*sslRedirBind, sslRedirectHandler{})
			log.Fatal("ssl redirect handler finished: ", err)
		}()
	}

	sigs := make(chan os.Signal, 1)
	signal.Notify(sigs, os.Interrupt, syscall.SIGTERM)

	sig := <-sigs
	log.Printf("Got signal %s, exiting.", sig)

	k8s.Stop()
}
