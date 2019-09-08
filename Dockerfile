from golang:1.13.0-alpine3.10 as build-env
run apk add --update git
arg GOPROXY
env CGO_ENABLED 0

workdir /src
add go.mod go.sum ./
run go mod download

add . ./
run go test ./...
run go install

from alpine:3.10
entrypoint ["/bin/kingress"]
copy --from=build-env /go/bin/* /bin/
