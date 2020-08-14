from mcluseau/golang-builder:1.15.0 as build
from alpine:3.12
entrypoint ["/bin/kingress"]
copy --from=build /go/bin/* /bin/
