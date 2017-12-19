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
kubectl expose deploy kingress --name kingress-http2https --port 80 --target-port 81 --external-ip=$ip
```

In standalone mode:

```
Usage of kingress:
  -alsologtostderr
    	log to standard error as well as files
  -change-apply-delay duration
    	Delay before applying change in Kubernetes configuration (default 100ms)
  -http string
    	HTTP bind specification (default ":80")
  -https string
    	HTTPS bind specification (default ":443")
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
  -resync-period duration
    	Period between full resyncs with Kubernetes (default 10m0s)
  -selector string
    	Ingress selector
  -ssl-redirect string
    	HTTP to HTTPS redirector bind specification (default ":81")
  -stderrthreshold value
    	logs at or above this threshold go to stderr
  -tls-secret string
    	Default TLS secret (format: namespace/name) (default "default/kingress-default")
  -v value
    	log level for V logs
  -vmodule value
    	comma-separated list of pattern=N settings for file-filtered logging
```
