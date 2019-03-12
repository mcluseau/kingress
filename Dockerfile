from golang:1.12.0-alpine3.9 as build-env
arg GOPROXY
env CGO_ENABLED 0

workdir /src
add go.mod go.sum ./
run go mod download

add . ./
run go test ./...
run go install

#run apk update && apk add gcc musl-dev
#env pkg github.com/mcluseau/kingress
#add . ${GOPATH}/src/${pkg}
#run cd ${GOPATH}/src/${pkg} \
# && go vet  ./... \
# && go test ./... \
# && go install

from alpine:3.9
entrypoint ["/bin/kingress"]
copy --from=build-env /go/bin/* /bin/
