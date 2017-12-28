package main

import (
	"crypto/tls"
	"log"
	"net/http"

	"github.com/mcluseau/kingress/config"
)

func startHTTPS(bind string) error {
	config := &tls.Config{
		GetCertificate: getCertificate,
	}

	listener, err := tls.Listen("tcp", bind, config)
	if err != nil {
		return err
	}

	log.Print("https: listening on ", bind)

	if err := http.Serve(listener, newHandler("https", portOfBind(bind))); err != nil {
		log.Fatal("https: serve error: ", err)
	}

	return nil
}

func getCertificate(helloInfo *tls.ClientHelloInfo) (*tls.Certificate, error) {
	certificate, ok := config.Current.HostCerts[helloInfo.ServerName]

	if !ok {
		log.Printf("https: using default certificate for %s", helloInfo.ServerName)
		if config.Current.DefaultCert == nil {
			log.Printf("https: warning: default certificate is not available")
		}
		return config.Current.DefaultCert, nil
	}

	return certificate, nil
}
