from mcluseau/golang-builder:1.24.2 as build
from alpine:3.21
entrypoint ["/bin/kingress"]
copy --from=build /go/bin/* /bin/
