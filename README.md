# kingress

Reverse HTTP proxy dedicated to Kubernetes Ingress

## Usage

This is intended to be run in a Kubernetes cluster, like that:

```
kubectl run kingress --image mcluseau/kingress

# expose HTTPS
kubectl expose deploy kingress --name kingress-https --port 443 --external-ip=$ip

# expose HTTP (following ingress rules)
kubectl expose deploy kingress --name kingress-http --port 80 --external-ip=$ip

# expose HTTP (always redirecting to HTTPS)
# you must add something like `-ssl-redirect=:81` to the container args
kubectl expose deploy kingress --name kingress-http2https --port 80 --target-port 81 --external-ip=$ip
```

## Supported annotations

This controller looks for annotations with the any of the following prefixes:

- `ingress.kubernetes.io`
- `nginx.ingress.kubernetes.io`

The following annotations are supported:

| Annotation | Description |
| --- | --- |
| `cors-allowed-origins` | comma separated list of CORS allowed origins. The special value '*' allows any origin. |
| `grpc` | handle gRPC requests |
| `grpc-web` | handle grpc-web requests |
| `secure-backends` | Make TLS connections to the upstream instead of plain HTTP. Initialy from ingress-nginx but removed from it, we still support it. |
| `ssl-redirect` | From [ingress-nginx](https://github.com/kubernetes/ingress-nginx/blob/master/docs/user-guide/nginx-configuration/annotations.md#server-side-https-enforcement-through-redirect) |
| `whitelist-source-range` | From [ingress-nginx](https://github.com/kubernetes/ingress-nginx/blob/master/docs/user-guide/nginx-configuration/annotations.md#whitelist-source-range) |

## Command line flags

```
$ ./kingress -h
Usage of ./kingress:
  -api string
    	API bind specication (empty to disable) (default "127.0.0.1:2287")
  -change-apply-delay duration
    	Delay before applying change in Kubernetes configuration (default 100ms)
  -custom string
    	Custom backend definitions (format: "<host><path>:<target IP>:<target port>,...")
  -debug-tls
    	activate TLS debug logs
  -flush-interval duration
    	forward flush interval.
    	If zero, no periodic flushing is done.
    	A negative value (ie: -1ns) means to flush immediately.
    	Ignored when a response is recognized as a streaming response; for such reponses, writes are flushed immediately. (default 10ms)
  -http string
    	HTTP bind specification (empty to disable) (default ":80")
  -https string
    	HTTPS bind specification (empty to disable) (default ":443")
  -kubeconfig string
    	Path to a kubeconfig. Only required if out-of-cluster. Defaults to envvar KUBECONFIG.
  -lb-hosts string
    	Load balancer hosts to write in ingress statuses (default "default")
  -master string
    	The address of the Kubernetes API server
  -namespace string
    	Namespace (defaults to all)
  -pprof-cpu string
    	Enable CPU profiling to this file
  -resync-period duration
    	Period between full resyncs with Kubernetes (default 10m0s)
  -selector string
    	Ingress selector
  -ssl-redirect string
    	HTTP to HTTPS redirector bind specification (empty to disable)
  -tls-secret string
    	Default TLS secret (format: namespace/name) (default "default/kingress-default")

```
