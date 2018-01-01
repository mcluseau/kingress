package main

import (
	"crypto/tls"
	"crypto/x509"
	"encoding/json"
	"log"
	"net/http"
	"time"

	"github.com/mcluseau/kingress/config"
)

func startAPI(apiBind string) error {
	log.Print("api: listening on ", apiBind)
	return http.ListenAndServe(apiBind, apiHandler{})
}

type apiHandler struct{}

func (_ apiHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case "GET":
		// ok

	default:
		http.NotFound(w, r)
		return
	}

	switch r.RequestURI {
	case "/config":
		writeConfig(w)

	default:
		http.NotFound(w, r)
		return
	}
}

func writeConfig(w http.ResponseWriter) {
	cfg := config.Current

	if cfg == nil {
		json.NewEncoder(w).Encode(nil)
		return
	}

	certs := make(map[string]*certInfo, len(cfg.HostCerts))
	for host, cert := range cfg.HostCerts {
		certs[host] = newCertInfo(cert)
	}

	json.NewEncoder(w).Encode(map[string]interface{}{
		"backends":            cfg.HostBackends,
		"default-certificate": newCertInfo(cfg.DefaultCert),
		"certificates":        certs,
	})
}

type certInfo struct {
	Defined   bool
	Error     error         `json:",omitempty"`
	NotAfter  time.Time     `json:",omitempty"`
	NotBefore time.Time     `json:",omitempty"`
	Issuer    []interface{} `json:",omitempty"`
	DNSNames  []string      `json:",omitempty"`
}

func newCertInfo(cert *tls.Certificate) *certInfo {
	if cert == nil || len(cert.Certificate) == 0 {
		return &certInfo{Defined: false}
	}

	xc, err := x509.ParseCertificate(cert.Certificate[0])
	if err != nil {
		return &certInfo{
			Defined: true,
			Error:   err,
		}
	}

	issuer := make([]interface{}, 0, len(xc.Issuer.Names))
	for _, name := range xc.Issuer.Names {
		issuer = append(issuer, name.Value)
	}

	return &certInfo{
		Defined:   true,
		NotAfter:  xc.NotAfter,
		NotBefore: xc.NotBefore,
		Issuer:    issuer,
		DNSNames:  xc.DNSNames,
	}
}
