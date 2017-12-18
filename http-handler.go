package main

import (
	"bufio"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"runtime"
	"strconv"
	"strings"

	"github.com/MikaelCluseau/kingress/config"
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
	if r.Method == http.MethodConnect {
		log.Print("%s: %v tried to use method %s", r.Proto, r.RemoteAddr, r.Method)
		return
	}

	var clientConn net.Conn

	defer func() {
		if err := recover(); err != nil {
			const size = 64 << 10
			buf := make([]byte, size)
			buf = buf[:runtime.Stack(buf, false)]
			log.Printf("%s: panic serving %v: %v\n%s", r.Proto, r.RemoteAddr, err, buf)
		}
	}()

	hijacker := w.(http.Hijacker)
	clientConn, _, err := hijacker.Hijack()
	if err != nil {
		writeError(r, clientConn, http.StatusServiceUnavailable)
		return
	}

	target, status := getBackend(r)
	if target == "" {
		// no backend matching
		writeError(r, clientConn, status)
		return
	}

	destConn, err := net.DialTimeout("tcp", target, dialTimeout)
	if err != nil {
		log.Print("%s: error serving %v to %v: ", err)
		writeError(r, clientConn, http.StatusBadGateway)
		return
	}

	log.Printf("endpoint=%s remote=%v host=%s target=%s %s %s %s", hh.Proto, r.RemoteAddr, r.Host, target, r.Method, r.RequestURI, r.Proto)

	if err = hh.writeHeaders(r, destConn); err != nil {
		log.Print("%s: error writing header from %v to %v: %v", r.Proto, r.RemoteAddr, target, err)
		writeError(r, clientConn, http.StatusBadGateway)
		return
	}

	go func() {
		io.Copy(clientConn, destConn)
		clientConn.Close()
	}()
	go func() {
		io.Copy(destConn, clientConn)
		destConn.Close()
	}()
}

func (hh *HttpHandler) writeHeaders(r *http.Request, out io.Writer) (err error) {
	if _, err = fmt.Fprintf(out, "%s %s %s\r\n", r.Method, r.URL.RequestURI(), r.Proto); err != nil {
		return
	}

	headers := http.Header{}
	headers.Add("Host", r.Host)

	for name, values := range r.Header {
		if name == "Forwarded" {
			continue
		}
		if strings.HasPrefix(name, "X-Forwarded-") {
			continue
		}
		for _, value := range values {
			headers.Add(name, value)
		}
	}

	// RFC7239: Forwarded HTTP Extension
	// https://tools.ietf.org/html/rfc7239#section-5
	headers.Add("X-Forwarded-For", r.RemoteAddr)
	headers.Add("X-Forwarded-Host", r.Host)
	headers.Add("X-Forwarded-Proto", hh.Proto)
	headers.Add("Forwarded", fmt.Sprintf("for=%s, host=%s, proto=%s", r.RemoteAddr, r.Host, hh.Proto))

	// end
	if err = headers.Write(out); err != nil {
		return
	}
	if _, err = out.Write([]byte{'\r', '\n'}); err != nil {
		return
	}
	return
}

func writeHeader(name, value string, buf *bufio.Writer) (err error) {
	if _, err = buf.WriteString(name + ": " + value + "\n"); err != nil {
		return
	}
	return
}

func writeError(r *http.Request, out io.WriteCloser, code int) {
	text := http.StatusText(code)
	fmt.Fprintf(out, "%s %d %s\n\n%s\n", r.Proto, code, text, text)
	out.Close()
}

// returns target and http status if no target is found
func getBackend(r *http.Request) (string, int) {
	backends := config.Current.HostBackends[r.Host]

	backendWithoutTarget := false
	for _, backend := range backends {
		if !backend.HandlesPath(r.RequestURI) {
			continue
		}

		target := backend.Target()
		if target == "" {
			backendWithoutTarget = true
			continue
		}

		return target, 0
	}

	if backendWithoutTarget {
		return "", http.StatusServiceUnavailable
	}

	return "", http.StatusNotFound
}
