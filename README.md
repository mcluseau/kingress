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
| `secure-backends` | Make TLS connections to the upstream instead of plain HTTP. Initialy from ingress-nginx but removed from it, we still support it. |
| `ssl-redirect` | From [ingress-nginx](https://github.com/kubernetes/ingress-nginx/blob/master/docs/user-guide/nginx-configuration/annotations.md#server-side-https-enforcement-through-redirect) |
| `whitelist-source-range` | From [ingress-nginx](https://github.com/kubernetes/ingress-nginx/blob/master/docs/user-guide/nginx-configuration/annotations.md#whitelist-source-range) |

## Command line flags

```
$ kingress -h
Usage of kingress:
  -alsologtostderr
    	log to standard error as well as files
  -api string
    	API bind specication (empty to disable) (default "127.0.0.1:2287")
  -change-apply-delay duration
    	Delay before applying change in Kubernetes configuration (default 100ms)
  -custom string
    	Custom backend definitions (format: "<host><path>:<target IP>:<target port>,...")
  -debug-tls
    	activate TLS debug logs
  -http string
    	HTTP bind specification (empty to disable) (default ":80")
  -https string
    	HTTPS bind specification (empty to disable) (default ":443")
  -httptest.serve string
    	if non-empty, httptest.NewServer serves on this address and blocks
  -log_backtrace_at value
    	when logging hits line file:N, emit a stack trace
  -log_dir string
    	If non-empty, write log files in this directory
  -logtostderr
    	log to standard error instead of files
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
  -stderrthreshold value
    	logs at or above this threshold go to stderr
  -tls-secret string
    	Default TLS secret (format: namespace/name) (default "default/kingress-default")
  -v value
    	log level for V logs
  -vmodule value
    	comma-separated list of pattern=N settings for file-filtered logging

```
