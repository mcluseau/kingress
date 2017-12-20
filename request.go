package main

import (
	"bytes"
	"crypto/tls"
	"encoding/base32"
	"fmt"
	"io"
	"log"
	"math/rand"
	"net"
	"net/http"
	"strings"
	"sync"
	"time"
)

type request struct {
	endpoint string
	id       string
	start    time.Time

	target proxyDestination

	proxyCount        int
	lock              sync.Mutex
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

func (r *request) dialTarget(target string, secure bool) (err error) {
	var c interface{}

	if secure {
		c, err = tls.Dial("tcp", target, &tls.Config{InsecureSkipVerify: true})
	} else {
		c, err = net.DialTimeout("tcp", target, dialTimeout)
	}
	if err != nil {
		return
	}
	r.target = c.(proxyDestination)
	return
}

type proxyDestination interface {
	io.Reader
	io.Writer
	io.Closer
	CloseWrite() error
}

func (r *request) proxy(counter *int64, dst proxyDestination, src io.Reader) {
	defer func() {
		dst.CloseWrite()

		r.lock.Lock()
		defer r.lock.Unlock()

		r.proxyCount -= 1

		if r.proxyCount != 0 {
			return
		}

		r.target.Close()
		r.logf("bytes-in=%d bytes-out=%d closed", r.bytesIn, r.bytesOut)
	}()

	r.lock.Lock()
	r.proxyCount += 1
	r.lock.Unlock()

	nb, err := io.Copy(dst, src)

	*counter += nb

	if err != nil {
		str := err.Error()

		if strings.Contains(str, "use of closed network connection") {
			return
		}

		r.logf("error: %s", str)
	}
}

func (req *request) writeHeaders(r *http.Request) (err error) {
	buf := bytes.NewBuffer(make([]byte, 0, 4096))

	if _, err = fmt.Fprintf(buf, "%s %s %s\r\n", r.Method, r.URL.RequestURI(), r.Proto); err != nil {
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
	headers.Add("X-Forwarded-Proto", req.endpoint)
	headers.Add("Forwarded", fmt.Sprintf("for=%s, host=%s, proto=%s", r.RemoteAddr, r.Host, req.endpoint))

	// end
	if err = headers.Write(buf); err != nil {
		return
	}
	if _, err = buf.Write([]byte{'\r', '\n'}); err != nil {
		return
	}

	req.bytesIn += int64(buf.Len())

	if _, err = buf.WriteTo(req.target); err != nil {
		return
	}
	return
}
