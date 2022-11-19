package main

import (
	"context"
	"crypto/tls"
	"io"
	"log"
	"sync"

	"github.com/mcluseau/kingress/config"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/credentials"
	"google.golang.org/grpc/metadata"
	"google.golang.org/grpc/peer"
)

var (
	errBackendUnavailable = grpc.Errorf(codes.Unavailable, "backend unavailable")
)

func (h *oxyHandler) grpc() *grpc.Server {
	if h.grpcSrv != nil {
		return h.grpcSrv
	}

	h.grpcL.Lock()
	defer h.grpcL.Unlock()

	if h.grpcSrv == nil {
		h.grpcSrv = grpc.NewServer(grpc.CustomCodec(rawPbCodec{}), grpc.UnknownServiceHandler(h.proxyGRPCStream))
	}

	return h.grpcSrv
}

type rawPbCodec struct{}

func (c rawPbCodec) Marshal(v interface{}) ([]byte, error) {
	return *(v.(*rawPb)), nil
}
func (c rawPbCodec) Unmarshal(data []byte, v interface{}) error {
	*(v.(*rawPb)) = data
	return nil
}
func (c rawPbCodec) String() string { return "proxy rawPbCodec" }

type rawPb []byte

func (h *oxyHandler) proxyGRPCStream(srv interface{}, src grpc.ServerStream) (err error) {
	log.Print("proxy GRPC stream")

	m, ok := grpc.MethodFromServerStream(src)
	if !ok {
		err = grpc.Errorf(codes.FailedPrecondition, "no method called")
		return
	}

	ctx := src.Context()

	peer, ok := peer.FromContext(ctx)
	if !ok {
		err = grpc.Errorf(codes.FailedPrecondition, "no peer in context")
		return
	}

	md, ok := metadata.FromIncomingContext(ctx)
	if !ok {
		err = grpc.Errorf(codes.FailedPrecondition, "no metadata in context")
		return
	}

	if _, ok := md["connection"]; ok {
		// this header prevent connection to the backend (https://github.com/improbable-eng/grpc-web/issues/568)
		md = md.Copy()
		delete(md, "connection")
	}

	backend := ctx.Value("backend").(*config.Backend)
	target := backend.Target()

	var tlsDialOpt grpc.DialOption

	if backend.Options.SecureBackends {
		tlsClientConfig := &tls.Config{
			InsecureSkipVerify: true,
		}
		tlsDialOpt = grpc.WithTransportCredentials(credentials.NewTLS(tlsClientConfig))
	} else {
		tlsDialOpt = grpc.WithInsecure()
	}

	conn, err := grpc.DialContext(ctx, target, tlsDialOpt, grpc.WithCodec(rawPbCodec{}))
	if err != nil {
		log.Print("failed to connect to ", target, ": ", err)
		return errBackendUnavailable
	}
	defer conn.Close()

	md.Set("x-forwarded-for", peer.Addr.String())

	clientCtx, cancel := context.WithCancel(ctx)
	defer cancel()
	clientCtx = metadata.NewOutgoingContext(clientCtx, md)

	clientStreamDesc := &grpc.StreamDesc{ServerStreams: true, ClientStreams: true}
	dst, err := grpc.NewClientStream(clientCtx, clientStreamDesc, conn, m)
	if err != nil {
		log.Print("failed to establish client stream: ", err)
		return errBackendUnavailable
	}

	wg := sync.WaitGroup{}
	wg.Add(2)

	go func() {
		defer wg.Done()

		md, hdrErr := dst.Header()
		if hdrErr != nil {
			err = hdrErr
			return
		}
		src.SendHeader(md)

		myErr := copyStream("s->c:", dst, src)

		src.SetTrailer(dst.Trailer())

		if myErr == io.EOF {
			return
		}
		err = myErr
	}()

	go func() {
		defer wg.Done()
		defer dst.CloseSend()

		myErr := copyStream("c->s:", src, dst)
		if myErr == io.EOF {
			return
		}
		err = myErr
	}()

	wg.Wait()

	return
}

func copyStream(flow string, src, dst grpc.Stream) (err error) {
	for {
		msg := &rawPb{}
		err = src.RecvMsg(msg)
		if err != nil {
			return
		}

		err = dst.SendMsg(msg)
		if err != nil {
			return
		}
	}
}
