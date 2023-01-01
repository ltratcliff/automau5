.DEFAULT_GOAL := build_windows

install_rust:
	#################### Installing Rust ########################
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
.PHONY:install_rust

src_setup: install_rust
	#################### Updating PATH ########################
	source $$HOME/.cargo/env
.PHONY:src_setup

install_cross: src_setup
	#################### Installing Cross ########################
	cargo install cross --git https://github.com/cross-rs/cross
.PHONY:install_cross

build_windows: install_cross
	#################### Windows Build ########################
	cross build --target x86_64-pc-windows-gnu --release
.PHONY:build_windows
