package main

import (
	"log"
	"net"
	"net/http"
	"time"
)

var (
	dialTimeout = 1 * time.Minute
)

func startHTTP(bind string) error {
	listener, err := net.Listen("tcp", bind)
	if err != nil {
		return err
	}

	log.Print("http: listening on ", bind)

	if err := http.Serve(listener, &HttpHandler{"http", portOfBind(bind)}); err != nil {
		log.Fatal("http: serve error: ", err)
	}

	return nil
}
