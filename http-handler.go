package main

import (
	"log"
	"net"
	"net/http"
	"strconv"
	"strings"

	"github.com/mcluseau/kingress/config"
)

var newHandler func(proto, port string) http.Handler = newOxyHandler

func portOfBind(bind string) string {
	addr, err := net.ResolveTCPAddr("tcp", bind)
	if err != nil {
		log.Fatal("bad bind: ", bind, ": ", err)
	}

	return strconv.Itoa(addr.Port)
}

// Returns "" iff the request can be forwarded to the backend, the reject reason otherwise
func allowRequest(backend *config.Backend, handlerProto string, w http.ResponseWriter, r *http.Request) string {
	// check for whitelist
	if backend.Options.WhitelistSourceRange != nil {
		host, _, err := net.SplitHostPort(r.RemoteAddr)
		if err != nil {
			panic(err) // not possible (built by net/http)
		}

		remoteIP := net.ParseIP(host)
		if remoteIP == nil {
			panic("remote IP shouldn't be nil") // not possible (IP is obtained from socket)
		}

		accessOk := false
		for _, ipnet := range backend.Options.WhitelistSourceRange {
			if ipnet.Contains(remoteIP) {
				accessOk = true
				break
			}
		}

		if !accessOk {
			http.Error(w, http.StatusText(http.StatusForbidden), http.StatusForbidden)
			return "rejecting (not in whitelist)"
		}
	}

	// check for SSL redirection
	if backend.Options.SSLRedirect && handlerProto != "https" {
		redirectToHTTPS(w, r)
		return "redirecting to HTTPS"
	}

	return ""
}

// returns target and http status if no target is found
func getBackend(r *http.Request) (*config.Backend, string, int) {
	hostWithoutPort := strings.Split(r.Host, ":")[0]
	backends := config.Current.HostBackends[hostWithoutPort]

	if backends == nil {
		// maybe a wildcard handles it
		if n := strings.Index(hostWithoutPort, "."); n > 0 {
			backends = config.Current.HostBackends["*"+hostWithoutPort[n:]]
		}
	}

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
