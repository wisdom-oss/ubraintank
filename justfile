
set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

target_dir := env_var_or_default("CARGO_TARGET_DIR", "target")

default:
    @just --list

generate-nvs:
    uv run python -m esp_idf_nvs_partition_gen generate --outdir {{target_dir}} config.csv config.nvs.bin 0x6000

write-nvs:
    uv run esptool --chip esp32 write_flash 0x9000 {{target_dir}}/config.nvs.bin

all: generate-nvs write-nvs
