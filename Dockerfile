FROM gcr.io/distroless/cc-debian13:debug
LABEL org.opencontainers.image.description="root filesystem"
COPY ./init.sh /init
COPY ./target/release/minimal ./target/release/httpserver ./target/release/httpserversync /bin/
