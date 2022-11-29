from mcluseau/golang-builder:1.19.2 as build
from alpine:3.16
entrypoint ["/bin/kingress"]
copy --from=build /go/bin/* /bin/
