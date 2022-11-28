package proxier

import (
	"crypto/tls"
	"io"
	"log"
	"net"
	"sync"
)

type ForwardHandler struct {
	Network string
	Target  string
}

func (h ForwardHandler) Handle(req Request, alreadyRead []byte, src net.Conn) (handled bool) {
	dst, err := net.Dial(h.Network, h.Target)

	return Forward(req, alreadyRead, src, dst, err, h.Network+"://"+h.Target+": ")
}

type TLSForwardHandler struct {
	Network string
	Target  string
	Config  *tls.Config
}

func (h TLSForwardHandler) Handle(req Request, alreadyRead []byte, src net.Conn) bool {
	dst, err := tls.Dial(h.Network, h.Target, h.Config)

	return Forward(req, alreadyRead, src, dst, err, h.Network+"://"+h.Target+": ")
}

func Forward(req Request, alreadyRead []byte, src, dst net.Conn, err error, logPrefix string) (handled bool) {
	handled = true

	logf := func(pattern string, values ...any) {
		if !Quiet {
			log.Printf(logPrefix+pattern, values...)
		}
	}

	if err != nil {
		logf("dial failed: %v", err)
		src.Write(BadGatewayResponse)
		return
	}

	defer dst.Close()

	_, err = dst.Write(alreadyRead)
	if err != nil {
		logf("write error: %v", err)
		return
	}

	var wg sync.WaitGroup
	wg.Add(2)

	go func() {
		defer wg.Done()
		io.Copy(dst, src)
		dst.Close()
	}()
	go func() {
		defer wg.Done()
		io.Copy(src, dst)
		src.Close()
	}()

	wg.Wait()

	return
}
