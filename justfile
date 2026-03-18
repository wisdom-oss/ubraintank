set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

target_dir := env_var_or_default("CARGO_TARGET_DIR", "target")
nvs_partition_offset := `uv run toml get --toml-path device.toml partition_data.nvs.offset`
nvs_partition_size := `uv run toml get --toml-path device.toml partition_data.nvs.size`

default:
    @just --list

generate-nvs:
    uv run python -m esp_idf_nvs_partition_gen generate --outdir {{target_dir}} config.csv config.nvs.bin {{nvs_partition_size}}

write-nvs:
    uv run esptool --chip esp32 write-flash {{nvs_partition_offset}} {{target_dir}}/config.nvs.bin

all: generate-nvs write-nvs
