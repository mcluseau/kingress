from mcluseau/golang-builder:1.14.6 as build
from alpine:3.12
entrypoint ["/bin/kingress"]
copy --from=build /go/bin/* /bin/
