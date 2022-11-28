package proxier

import (
	"bytes"
	"io"
	"log"
	"net"
	"net/http"
	"net/textproto"
	"strconv"
	"sync"
	"time"
)

var (
	// Quiet prevents any log from this package.
	Quiet bool

	// Verbose makes proxiers log every client communication error.
	Verbose bool

	// Debug makes proxiers log details about requests handling.
	Debug bool

	// DefaultReadLimit is the maximum size of the header to read before failing.
	// The default value is 16kiB
	DefaultReadLimit int64 = 16 << 10

	// InvalidProtocolResponse is the response sent to a client that did not respect the basics of HTTP.
	InvalidProtocolResponse = HTTPStatusResponse(http.StatusBadRequest, "")

	// InvalidHostResponse is the response sent to a client when the required host does not match the request's "Host" header.
	InvalidHostResponse = HTTPStatusResponse(http.StatusBadRequest, "Invalid host")

	// NoHandlerResponse is the response sent to a client when no handler could handle the request.
	NoHandlerResponse = HTTPStatusResponse(http.StatusNotFound, "")

	// BadGatewayResponse is the response sent to a client when no handler could handle the request.
	BadGatewayResponse = HTTPStatusResponse(http.StatusBadGateway, "")
)

func HTTPStatusResponse(statusCode int, text string) []byte {
	status := http.StatusText(statusCode)
	if text == "" {
		text = status
	}

	return []byte("HTTP/1.1 " + strconv.Itoa(statusCode) + " " + status + "\r\n" +
		"Content-Type: text/plain; charset=utf-8\r\n" +
		"Connection: close\r\n" +
		"\r\n" +
		text + "\r\n")
}

type Request struct {
	Method      string
	Path        string
	Host        string
	HTTPVersion string
}

type Handler interface {
	Handle(req Request, alreadyRead []byte, conn net.Conn) bool
}

func New() *Proxier {
	return &Proxier{
		ReadLimit:   DefaultReadLimit,
		ReadTimeout: 30 * time.Second,
	}
}

type Proxier struct {
	// ReadLimit imposes a maximum size for the request header (or until the "Host" header is `ReadMinimum=true`).
	// Defauts to `proxier.DefaultReadLimit`
	ReadLimit int64

	// ReadTimeout imposes a maximum time to read the header (0 for no time limit).
	ReadTimeout time.Duration

	// ReadMinimum limits reads to the minimum amount required. Otherwise, read the full header before passing
	// the connection to the handlers. If your handlers need details in headers and this is set to false (the default),
	// use `http.ReadRequest(alreadyRead)` to build get the standard request object.
	ReadMinimum bool

	l        sync.Mutex
	handlers []Handler
}

// Handle a incoming connection.
// If `requireHost` is not empty, return a bad request if the "Host" header does not match.
// This allows to force consistency between the Host header and the TLS host requested.
func (p *Proxier) Handle(conn net.Conn, requireHost string) {
	defer conn.Close()

	if timeout := p.ReadTimeout; timeout != 0 {
		conn.SetReadDeadline(time.Now().Add(timeout))
	}

	reader := io.LimitReader(conn, p.ReadLimit)

	allRead := make([]byte, 0, 4096)
	buf := make([]byte, 4096)

	req := Request{}

	lineStart := 0
	onRequestLine := true

readLoop:
	for {
		n, err := reader.Read(buf)
		if err != nil {
			if Verbose {
				log.Print("read failed in connection from ", conn.RemoteAddr(), ": ", err)
			}
			return
		}

		pos := len(allRead)
		allRead = append(allRead, buf[:n]...)

		for {
			lfIdx := bytes.IndexByte(allRead[pos:], '\n')
			if lfIdx == -1 {
				continue // line not finished yet
			}

			line := allRead[lineStart : pos+lfIdx]
			if lastIdx := len(line) - 1; lastIdx != -1 && line[lastIdx] == '\r' {
				line = line[:lastIdx]
			}

			if len(line) == 0 {
				break readLoop // header finished
			}

			pos += lfIdx + 1
			lineStart = pos

			if onRequestLine {
				if Debug {
					log.Printf("request line: %q", string(line))
				}

				method, after, found := bytes.Cut(line, []byte{' '})
				if !found {
					break readLoop // protocol not respected
				}

				requestURI, httpVersion, found := bytes.Cut(after, []byte{' '})
				if !found {
					break readLoop // protocol not respected
				}

				req.Method = string(method)

				requestURI, _, _ = bytes.Cut(requestURI, []byte{'?'}) // cut request URI before query parameters, if any
				req.Path = string(requestURI)

				req.HTTPVersion = string(httpVersion)

				onRequestLine = false
				continue
			}

			if Debug {
				log.Printf("header line:  %q", string(line))
			}

			header, value, found := bytes.Cut(line, []byte{':'})
			if !found {
				continue // we can ignore, the backend will see what to do with that
			}

			if req.Host != "" && textproto.CanonicalMIMEHeaderKey(string(header)) == "Host" {
				req.Host = string(bytes.TrimSpace(value))

				if p.ReadMinimum {
					// we have everything we want, stop reading here
					break readLoop
				}
			}
		}
	}

	conn.SetReadDeadline(time.Time{})

	if Debug {
		log.Printf("request from %s: %+v", conn.RemoteAddr(), req)
	}

	if onRequestLine {
		// invalid at the protocol level
		if Verbose {
			log.Print("invalid protocol (request line) in connection from ", conn.RemoteAddr())
		}
		conn.Write(InvalidProtocolResponse)
		return
	}

	if requireHost != "" && req.Host != requireHost {
		// invalid at the protocol level
		if Verbose {
			log.Printf("invalid host in connection from %s: expected %q, got %q", conn.RemoteAddr(), requireHost, req.Host)
		}
		conn.Write(InvalidHostResponse)
		return
	}

	// go through the handlers

	p.l.Lock()
	handlers := p.handlers
	p.l.Unlock()

	for i, handler := range handlers {
		if handler.Handle(req, allRead, conn) {
			if Debug {
				log.Printf("request from %s handled by handler %d", conn.RemoteAddr(), i)
			}
			return
		}
	}

	if Verbose {
		log.Printf("no handler took connection from %s (request: %+v)", conn.RemoteAddr(), req)
	}
	conn.Write(NoHandlerResponse)
}

func (p *Proxier) AddHandlers(handlers ...Handler) {
	p.l.Lock()
	p.handlers = append(p.handlers, handlers...)
	p.l.Unlock()
}

func (p *Proxier) SetHandlers(handlers []Handler) {
	p.l.Lock()
	p.handlers = handlers
	p.l.Unlock()
}

func (p *Proxier) CopyHandlers(handlers []Handler) {
	handlersCopy := make([]Handler, len(handlers))
	copy(handlersCopy, handlers)

	p.SetHandlers(handlersCopy)
}
