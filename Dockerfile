from mcluseau/golang-builder:1.21.6 as build
from alpine:3.19
entrypoint ["/bin/kingress"]
copy --from=build /go/bin/* /bin/
