setup:
	cargo install diesel_cli --no-default-features --features "postgres"

build:
	cargo build

db_migrate:
	diesel migration run

server:
	cargo run -p server
