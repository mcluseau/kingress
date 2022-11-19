package main

import (
	"context"
	"crypto/tls"
	"flag"
	"log"
	"net"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/improbable-eng/grpc-web/go/grpcweb"
	"github.com/mcluseau/kingress/config"
	"github.com/vulcand/oxy/forward"
	"google.golang.org/grpc"
)

var (
	tlsConfig = &tls.Config{
		InsecureSkipVerify: true,
	}

	flushInterval = flag.Duration("flush-interval", 10*time.Millisecond, `forward flush interval.
If zero, no periodic flushing is done.
A negative value (ie: -1ns) means to flush immediately.
Ignored when a response is recognized as a streaming response; for such reponses, writes are flushed immediately.`)
)

type oxyHandler struct {
	Proto string
	Port  string

	options config.BackendOptions

	fwd *forward.Forwarder

	grpcL      sync.Mutex
	grpcwebL   sync.Mutex
	grpcSrv    *grpc.Server
	grpcwebSrv *grpcweb.WrappedGrpcServer
}

func newHandler(proto, port string) http.Handler {
	fwd, err := forward.New(
		forward.PassHostHeader(true),
		forward.RoundTripper(roundTripper()),
		forward.WebsocketTLSClientConfig(tlsConfig),
		forward.Stream(true),
		forward.StreamingFlushInterval(*flushInterval),
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

	log.Print(r.Header)
	log.Print("CT: ", r.Header.Get("content-type"))

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

	defer func() {
		logCh <- &RequestEndLog{
			Request: req,
			Time:    req.Clock(),
		}
	}()

	r.URL.Host = backend.Target()
	r.URL.Scheme = "http"

	if backend.Options.SecureBackends {
		r.URL.Scheme = "https"
	}

	r = r.WithContext(context.WithValue(r.Context(), "backend", backend))

	if backend.Options.GRPCWeb &&
		r.Method == http.MethodPost &&
		strings.HasPrefix(r.Header.Get("content-type"), "application/grpc-web") {
		// handle gRPC-web request
		h.grpcweb().ServeHTTP(w, r)
		return
	}

	if backend.Options.GRPC &&
		r.ProtoMajor == 2 &&
		strings.HasPrefix(r.Header.Get("Content-Type"), "application/grpc") {
		// handle gRPC request
		h.grpc().ServeHTTP(w, r)
		return
	}

	h.fwd.ServeHTTP(w, r)
}

func (h *oxyHandler) grpcweb() *grpcweb.WrappedGrpcServer {
	if h.grpcwebSrv != nil {
		return h.grpcwebSrv
	}

	h.grpcwebL.Lock()
	defer h.grpcwebL.Unlock()

	if h.grpcwebSrv == nil {
		grpcSrv := h.grpc()
		h.grpcwebSrv = grpcweb.WrapServer(grpcSrv)
	}

	return h.grpcwebSrv
}
