# ffi_bind
## Build and Run ffi-bind
### Build
RUSTFLAGS='-L ./lib' cargo build

### Run
LD_LIBRARY_PATH="./lib" target/debug/ffibind

## Run echo.sh
`echo.sh`