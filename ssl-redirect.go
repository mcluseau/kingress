package main

import (
	"net/http"
)

type sslRedirectHandler struct{}

func (_ sslRedirectHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	redirectToHTTPS(w, r)
}

func redirectToHTTPS(w http.ResponseWriter, r *http.Request) {
	u := *r.URL
	u.Scheme = "https"
	u.Host = r.Host

	http.Redirect(w, r, u.String(), http.StatusMovedPermanently)
}
