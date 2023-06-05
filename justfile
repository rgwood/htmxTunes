watch:
    watchexec --exts=rs,js,html,css --on-busy-update=restart -- cargo run

run:
    cargo run

test:
    cargo test

build-release:
    cargo build --release
    ls target/release
