package main

import (
	"encoding/base32"
	"log"
	"math/rand"
	"time"
)

type request struct {
	endpoint string
	id       string
	start    time.Time

	bytesIn, bytesOut int64
}

func newRequest(endpoint string) *request {
	idBytes := make([]byte, 10)
	if _, err := rand.Read(idBytes); err != nil {
		panic(err)
	}

	return &request{
		endpoint: endpoint,
		id:       base32.StdEncoding.EncodeToString(idBytes),
		start:    time.Now(),
	}
}

func (r *request) logf(pattern string, args ...interface{}) {
	time := time.Since(r.start)

	allArgs := append([]interface{}{r.endpoint, r.id, time, time.Nanoseconds()}, args...)
	log.Printf("endpoints=%s req=%s time=%s time-ns=%d "+pattern, allArgs...)
}
