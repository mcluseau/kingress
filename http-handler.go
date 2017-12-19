package main

import (
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"runtime"
	"strconv"

	"github.com/mcluseau/kingress/config"
)

func portOfBind(bind string) string {
	addr, err := net.ResolveTCPAddr("tcp", bind)
	if err != nil {
		log.Fatal("bad bind: ", bind, ": ", err)
	}

	return strconv.Itoa(addr.Port)
}

type HttpHandler struct {
	Proto string
	Port  string
}

func (hh *HttpHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodConnect:
		log.Printf("%s: %v tried to use method %s", r.Proto, r.RemoteAddr, r.Method)
		return
	}

	req := newRequest(hh.Proto)

	var clientConn net.Conn

	defer func() {
		if err := recover(); err != nil {
			const size = 64 << 10
			buf := make([]byte, size)
			buf = buf[:runtime.Stack(buf, false)]
			req.logf("panic: %v\n%s", err, buf)
		}
	}()

	backend, target, status := getBackend(r)
	if target == "" {
		// no backend matching
		http.Error(w, http.StatusText(status), status)
		return
	}

	req.logf("remote=%s host=%s ingress=%s target=%s method=%s uri=%q proto=%q",
		r.RemoteAddr, r.Host, backend.IngressRef, target, r.Method, r.RequestURI, r.Proto)

	hijacker := w.(http.Hijacker)
	clientConn, clientRW, err := hijacker.Hijack()
	if err != nil {
		req.logf("hijack error: %s", err)
		http.Error(w, http.StatusText(http.StatusInternalServerError), http.StatusInternalServerError)
		return
	}

	destConn, err := net.DialTimeout("tcp", target, dialTimeout)
	if err != nil {
		req.logf("dial error: %s", err)
		writeError(r, clientConn, http.StatusBadGateway)
		return
	}

	if err = req.writeHeaders(r, destConn); err != nil {
		req.logf("error writing headers: %s", err)
		writeError(r, clientConn, http.StatusBadGateway)
		return
	}

	go req.copy(&req.bytesOut, clientRW, destConn, clientConn)
	go req.copy(&req.bytesIn, destConn, clientRW, destConn)
}

func writeError(r *http.Request, out io.WriteCloser, code int) {
	text := http.StatusText(code)
	fmt.Fprintf(out, "%s %d %s\r\n\r\n%s\n", r.Proto, code, text, text)
	out.Close()
}

// returns target and http status if no target is found
func getBackend(r *http.Request) (*config.Backend, string, int) {
	backends := config.Current.HostBackends[r.Host]

	for _, backend := range backends {
		if !backend.HandlesPath(r.RequestURI) {
			continue
		}

		target := backend.Target()
		if target == "" {
			return nil, "", http.StatusServiceUnavailable
		}

		return backend, target, 0
	}

	return nil, "", http.StatusNotFound
}
