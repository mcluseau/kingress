package main

import (
	"crypto/tls"
	"log"
	"net"
	"net/http"
	"time"

	"github.com/vulcand/oxy/forward"
)

var (
	tlsConfig = &tls.Config{
		InsecureSkipVerify: true,
	}
)

type oxyHandler struct {
	Proto string
	Port  string
	fwd   *forward.Forwarder
}

func newOxyHandler(proto, port string) http.Handler {
	fwd, err := forward.New(
		forward.PassHostHeader(true),
		forward.RoundTripper(roundTripper()),
		forward.WebsocketTLSClientConfig(tlsConfig),
		forward.Stream(true),
	)
	if err != nil {
		panic(err) // what can it be?
	}

	return &oxyHandler{
		Proto: proto,
		Port:  port,

		fwd: fwd,
	}
}

func roundTripper() http.RoundTripper {
	return &http.Transport{
		TLSClientConfig: tlsConfig,
		// below are the defaults
		Proxy: http.ProxyFromEnvironment,
		DialContext: (&net.Dialer{
			Timeout:   30 * time.Second,
			KeepAlive: 30 * time.Second,
			DualStack: true,
		}).DialContext,
		MaxIdleConns:          100,
		IdleConnTimeout:       90 * time.Second,
		TLSHandshakeTimeout:   10 * time.Second,
		ExpectContinueTimeout: 1 * time.Second,
	}
}

func (h *oxyHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodConnect:
		log.Printf("%s: %v tried to use method %s", r.Proto, r.RemoteAddr, r.Method)
		return
	}

	req := newRequest(h.Proto)

	backend, target, status := getBackend(r)
	if status != 0 {
		// no backend matching
		http.Error(w, http.StatusText(status), status)
		return
	}

	startLog := &RequestStartLog{
		Request: req,
		Remote:  r.RemoteAddr,
		Proto:   r.Proto,
		Host:    r.Host,
		Method:  r.Method,
		URI:     r.RequestURI,
		Ingress: backend.IngressRef,
		Target:  target,
		Reject:  allowRequest(backend, h.Proto, w, r),
	}

	logCh <- startLog

	if len(startLog.Reject) != 0 {
		return
	}

	r.URL.Host = backend.Target()
	r.URL.Scheme = "http"

	if backend.Options.SecureBackends {
		r.URL.Scheme = "https"
	}

	h.fwd.ServeHTTP(w, r)

	logCh <- &RequestEndLog{
		Request: req,
		Time:    req.Clock(),
	}
}
