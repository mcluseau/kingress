from mcluseau/golang-builder:1.25.3 as build
from alpine:3.22
entrypoint ["/bin/kingress"]
copy --from=build /go/bin/* /bin/
