from mcluseau/golang-builder:1.27.3 as build

from alpine:3.24.3
entrypoint ["/bin/kingress"]
copy --from=build /go/bin/* /bin/
