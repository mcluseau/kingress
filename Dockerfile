from golang:1.11.1-alpine3.8 as build-env
run apk update && apk add gcc musl-dev
env pkg github.com/mcluseau/kingress
add . ${GOPATH}/src/${pkg}
run cd ${GOPATH}/src/${pkg} \
 && go vet  ./... \
 && go test ./... \
 && go install

from alpine:3.8
entrypoint ["/kingress"]
copy --from=build-env /go/bin/kingress /
