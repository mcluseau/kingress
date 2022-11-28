package main

import (
	"crypto/tls"
	"flag"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"

	"github.com/mcluseau/kingress/proxier"
)

var (
	stayOpen = false
	bind     = "[::1]:2480"
)

func main() {
	flag.BoolVar(&stayOpen, "stay-open", stayOpen, "stay open after testing")
	flag.StringVar(&bind, "bind", bind, "bind address")

	flag.Parse()

	proxier.Verbose = true
	proxier.Debug = true

	proxy := proxier.New()

	proxy.AddHandlers(proxier.TLSForwardHandler{
		Config: &tls.Config{
			InsecureSkipVerify: true,
		},
		Network: "tcp",
		Target:  "127.0.0.1:443",
	})

	l, err := net.Listen("tcp", bind)
	if err != nil {
		log.Fatal("failed to listen: ", err)
	}

	go func() {
		for {
			conn, err := l.Accept()
			if err != nil {
				break
			}

			go proxy.Handle(conn, "")
		}
	}()

	if stayOpen {
		go test()
		select {}
	}

	if !test() {
		os.Exit(1)
	}
}

func test() bool {
	resp, err := http.Get("http://" + bind + "/test?param=value")
	if err != nil {
		log.Print("GET dial failed: ", err)
		return false
	}

	log.Print("response status: ", resp.Status)
	log.Print("response header: ", resp.Header)
	log.Print("response body:")

	_, err = io.Copy(os.Stdout, resp.Body)
	os.Stdout.Sync()

	if err != nil {
		log.Print("failed to fully read the response: ", err)
		return false
	}

	fmt.Println("-- end of response --")

	return true
}
