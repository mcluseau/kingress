from mcluseau/golang-builder:1.25.4 as build
from alpine:3.22.2
entrypoint ["/bin/kingress"]
copy --from=build /go/bin/* /bin/
