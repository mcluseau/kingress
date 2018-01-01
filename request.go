package main

import (
	"encoding/base32"
	"math/rand"
	"time"
)

type request struct {
	Endpoint string
	ID       string
	start    time.Time
}

func newRequest(endpoint string) *request {
	idBytes := make([]byte, 10)
	if _, err := rand.Read(idBytes); err != nil {
		panic(err)
	}

	return &request{
		Endpoint: endpoint,
		ID:       base32.StdEncoding.EncodeToString(idBytes),
		start:    time.Now(),
	}
}

func (r *request) Clock() time.Duration {
	return time.Since(r.start)
}

func (r *request) ToLog(message *LogMessage) {
	message.
		Field("endpoint", r.Endpoint).
		Field("req", r.ID)
}

type RequestStartLog struct {
	Request *request
	Remote  string
	Proto   string
	Host    string
	Method  string
	URI     string
	Ingress string
	Target  string
	Reject  string
}

var _ Loggable = &RequestStartLog{}

func (l *RequestStartLog) ToLog(message *LogMessage) {
	l.Request.ToLog(message)

	message.
		Field("start", l.Request.start).
		Field("remote", l.Remote).
		Field("proto", l.Proto).
		Field("host", l.Host).
		Field("method", l.Method).
		Field("uri", l.URI).
		Field("ingress", l.Ingress).
		Field("target", l.Target)

	if len(l.Reject) != 0 {
		message.Field("reject", l.Reject)
	}
}

type RequestEndLog struct {
	Request *request
	Time    time.Duration
	Error   string
}

var _ Loggable = &RequestEndLog{}

func (l *RequestEndLog) ToLog(message *LogMessage) {
	l.Request.ToLog(message)

	message.
		Field("time", l.Time).
		Field("time-ns", l.Time.Nanoseconds())

	if len(l.Error) != 0 {
		message.Field("error", l.Error)
	}
}
