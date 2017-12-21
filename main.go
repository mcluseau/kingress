package main

import (
	"flag"
	"log"
	"math/rand"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/mcluseau/kingress/k8s"
)

var (
	httpBind     = flag.String("http", ":80", "HTTP bind specification (empty to disable)")
	httpsBind    = flag.String("https", ":443", "HTTPS bind specification (empty to disable)")
	sslRedirBind = flag.String("ssl-redirect", "", "HTTP to HTTPS redirector bind specification (empty to disable)")
)

func main() {
	flag.Set("logtostderr", "true")
	flag.Parse()

	log.Print("Starting...")

	// TODO init from crypto/rand
	rand.Seed(time.Now().UnixNano())

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
	os.Exit(0)
}
