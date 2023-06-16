FROM ghcr.io/rust-cross/rust-musl-cross:x86_64-musl as builder
WORKDIR /home/rust/src
COPY . .
RUN cargo build --release --bin controller --target x86_64-unknown-linux-musl

FROM scratch
COPY --from=builder /home/rust/src/target/x86_64-unknown-linux-musl/release/controller /
COPY ./log4rs.yml /
EXPOSE 8080
ENTRYPOINT ["/controller"]