package config

import (
	"crypto/tls"
	"sync"
)

type Backends map[string][]*Backend

type Certificates map[string]*tls.Certificate

type Config struct {
	HostBackends Backends
	HostCerts    Certificates
	DefaultCert  *tls.Certificate
}

var (
	Current = &Config{}
	mutex   = sync.Mutex{}
)

func Lock() {
	mutex.Lock()
}

func Unlock() {
	mutex.Unlock()
}
