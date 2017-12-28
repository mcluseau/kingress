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

// Returns true iff the request can be forwarded to the backend
func allowRequest(backend *config.Backend, handlerProto string, req *request, w http.ResponseWriter, r *http.Request) bool {
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
			req.logf("rejecting (not in whitelist)")
			http.Error(w, http.StatusText(http.StatusForbidden), http.StatusForbidden)
			return false
		}
	}

	// check for SSL redirection
	if backend.Options.SSLRedirect && handlerProto == "http" {
		req.logf("redirecting to HTTPS")
		redirectToHTTPS(w, r)
		return false
	}

	return true
}

// returns target and http status if no target is found
func getBackend(r *http.Request) (*config.Backend, string, int) {
	hostWithoutPort := strings.Split(r.Host, ":")[0]
	backends := config.Current.HostBackends[hostWithoutPort]

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
