from mcluseau/rust:1.86.0 as build

workdir /app
copy . .

run --mount=type=cache,id=rust-alpine-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=rust-alpine-target,sharing=private,target=/app/target \
    cargo install --root=/dist --path .

from alpine:3.21
entrypoint ["/bin/kingress"]
copy --from=build /dist/* /bin/
