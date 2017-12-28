package main

import (
	"crypto/tls"
	"log"
	"net"
	"net/http"
	"runtime"
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

	defer func() {
		if err := recover(); err != nil {
			const size = 64 << 10
			buf := make([]byte, size)
			buf = buf[:runtime.Stack(buf, false)]
			req.logf("panic: %v\n%s", err, buf)
		}
	}()

	backend, target, status := getBackend(r)
	if status != 0 {
		// no backend matching
		http.Error(w, http.StatusText(status), status)
		return
	}

	req.logf("remote=%s host=%s ingress=%s target=%s method=%s uri=%q proto=%q",
		r.RemoteAddr, r.Host, backend.IngressRef, target, r.Method, r.RequestURI, r.Proto)

	if !allowRequest(backend, h.Proto, req, w, r) {
		return
	}

	r.URL.Host = backend.Target()
	r.URL.Scheme = "http"

	if backend.Options.SecureBackends {
		r.URL.Scheme = "https"
	}

	h.fwd.ServeHTTP(w, r)

	req.logf("finished")
}
