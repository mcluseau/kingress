package main

import (
	"errors"
	"log"
	"net"
	"net/http"
	"time"
)

var (
	headerReadTimeout = 1 * time.Minute
	timeZero          = time.Time{}
	dialTimeout       = 1 * time.Minute

	ErrBadRequestLine = errors.New("request line too large")
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
