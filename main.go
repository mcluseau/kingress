package main

import (
	"flag"
	"log"
	"math/rand"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/mcluseau/kingress/k8s"
)

var (
	httpBind  = flag.String("http", ":80", "HTTP bind specification")
	httpsBind = flag.String("https", ":443", "HTTPS bind specification")
)

func main() {
	flag.Set("logtostderr", "true")
	flag.Parse()

	log.Print("Starting...")

	// TODO init from crypto/rand
	rand.Seed(time.Now().UnixNano())

	k8s.Start()

	go func() {
		err := startHTTP(*httpBind)
		log.Fatal("http handler finished: ", err)
	}()

	go func() {
		err := startHTTPS(*httpsBind)
		log.Fatal("https handler finished: ", err)
	}()

	sigs := make(chan os.Signal, 1)
	signal.Notify(sigs, os.Interrupt, syscall.SIGTERM)

	sig := <-sigs
	log.Printf("Got signal %s, exiting.", sig)

	k8s.Stop()
	os.Exit(0)
}
