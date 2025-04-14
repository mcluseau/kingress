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
$ target/debug/kingress -h
Usage: kingress [OPTIONS]

Options:
  -n, --namespace <NAMESPACE>            
      --no-api                           
      --http <HTTP>                      [default: [::]:80]
      --https <HTTPS>                    [default: [::]:443]
      --api <API>                        [default: [::1]:2287]
      --cluster-domain <CLUSTER_DOMAIN>  
  -h, --help                             Print help
  -V, --version                          Print version

```
