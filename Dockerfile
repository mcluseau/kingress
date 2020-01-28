from mcluseau/golang-builder:1.13.6 as build
from alpine:3.10
entrypoint ["/bin/kingress"]
copy --from=build /go/bin/* /bin/
