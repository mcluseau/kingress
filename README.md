# kingress

Reverse HTTP proxy dedicated to Kubernetes Ingress

## Usage

This is intended to be run in a Kubernetes cluster, like that:

```
kubectl run kingress --image mcluseau/kingress

# expose HTTPS
kubectl expose deploy kingress --name kingress-https --port 443 --external-ip=$ip

# expose HTTP
kubectl expose deploy kingress --name kingress-http --port 80 --external-ip=$ip
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
| `http2` | Allow HTTP/2 in TLS connections (works only for ingresses with a single '\*' match) |

## Command line flags

```
$ ./kingress --help
Usage: kingress [OPTIONS]

Options:
  -n, --namespace <NAMESPACE>
          Kubernetes namespace to watch. All namespaces are watched if not set

      --no-api
          Disable the kingress API to check internal state

      --api <API>
          API server bind address
          
          [default: [::1]:2287]

      --http <HTTP>
          HTTP server bind address
          
          [default: [::]:80]

      --https <HTTPS>
          HTTPS server bind address
          
          [default: [::]:443]

      --resolver <RESOLVER>
          Method to resolve service endpoints
          
          [default: kube]

          Possible values:
          - dns-host: make DNS A queries
          - kube:     ask kube-apiserver

      --resolver-cache-size <RESOLVER_CACHE_SIZE>
          Size of the resolver cache. 0 disables caching
          
          [default: 256]

      --resolver-cache-expiry <RESOLVER_CACHE_EXPIRY>
          Resolutions expiration delay in seconds
          
          [default: 5]

      --resolver-cache-negative-expiry <RESOLVER_CACHE_NEGATIVE_EXPIRY>
          Failed resolutions expiration delay in seconds
          
          [default: 1]

      --cluster-domain <CLUSTER_DOMAIN>
          DNS suffix used by the dns-host resolver to form service FQDNs. If not set, rely on resolv.conf

      --kube-zone <KUBE_ZONE>
          Zone used by the kube resolver to filter endpoints, if set

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version

```
