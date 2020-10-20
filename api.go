package main

import (
	"crypto/tls"
	"crypto/x509"
	"encoding/json"
	"fmt"
	"html/template"
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

	switch r.URL.Path {
	case "/":
		writeStatus(w)

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

var statusTemplate = template.Must(template.New("status").
	Funcs(template.FuncMap{
		"certColor": func(info certInfo) string {
			expiresIn := info.NotAfter.Sub(time.Now())

			if expiresIn < 0 {
				return "danger"
			} else if expiresIn < 7*24*time.Hour {
				return "warning"
			}
			return "success"
		},
	}).Parse(`
<h2>Certificates</h2>
<table class="table">
<thead><tr>
    <th>Host</th>
    <th>Not after</th>
    <th>Not before</th>
    <th>Issuer</th>
    <th>DNS names</th>
</tr></thead>
<tbody>
{{ with .DefaultCertificate }}{{ if . }}
<tr><td><strong>default</strong></td>
    <td class="bg-{{ certColor . }}">{{ .NotAfter }}</td>
    <td>{{ .NotBefore }}</td>
    <td>{{ range .Issuer   }}<span class="badge badge-info">{{.}}</span> {{end}}</td>
    <td>{{ range .DNSNames }}<span class="badge badge-info">{{.}}</span> {{end}}</td>
</tr>
{{ end }}{{ end }}
{{ range $n, $c := .Certificates }}
<tr><td>{{ $n }}</td>
    <td class="bg-{{ certColor . }}">{{ .NotAfter }}</td>
    <td>{{ .NotBefore }}</td>
    <td>{{ range .Issuer   }}<span class="badge badge-info">{{.}}</span> {{end}}</td>
    <td>{{ range .DNSNames }}<span class="badge badge-info">{{.}}</span> {{end}}</td>
</tr>
{{ end }}
</tbody></table>

<h2>Backends</h2>
<table class="table">
<thead><tr>
    <th>Host</th>
    <th>Path prefix</th>
    <th>Ingress</th>
    <th>Options</th>
    <th>Targets</th>
</tr></thead>
<tbody>
{{ range $host, $b := .Backends }}
{{ range . }}
<tr><td>{{ $host }}</td>
    <td>{{ .Prefix }}</td>
    <td>{{ .IngressRef }}</td>
    <td>{{ range $k, $v := .Options.Get }}{{if .}}<span class="badge badge-info">{{$k}}{{ if ne true . }}:{{.}}{{end}}</span> {{end}}{{end}}</td>
    <td>{{ range .Targets }}<span class="badge badge-info">{{.}}</span> {{end}}</td>
</tr>
{{ end }}{{ end }}
</tbody></table>
`))

func writeStatus(w http.ResponseWriter) {
	fmt.Fprint(w, `<!doctype html>
<html><head><title>kingress status</title>
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/bootstrap@4.5.3/dist/css/bootstrap.min.css" integrity="sha384-TX8t27EcRE3e/ihU7zmQxVncDAy5uIKz4rEkgIXeMed4M0jlfIDPvg6uqKI2xXr2" crossorigin="anonymous">
</head><body>`)

	defer fmt.Fprint(w, `</body></html>`)

	cfg := config.Current

	if cfg == nil {
		fmt.Fprint(w, "<h2>not yet configured</h2>")
		return
	}

	certs := make(map[string]*certInfo, len(cfg.HostCerts))
	for host, cert := range cfg.HostCerts {
		certs[host] = newCertInfo(cert)
	}

	err := statusTemplate.Execute(w, map[string]interface{}{
		"Backends":           cfg.HostBackends,
		"DefaultCertificate": newCertInfo(cfg.DefaultCert),
		"Certificates":       certs,
	})

	if err != nil {
		fmt.Fprintf(w, "<div class=\"alert alert-danger\">Render error: %v</div>", err)
	}
}
