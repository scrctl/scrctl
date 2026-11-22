find_package(PkgConfig REQUIRED)
pkg_check_modules(SDL3 REQUIRED sdl3>=3.4)
find_program(SCRCTL_CARGO cargo REQUIRED)
set(scrctl_cargo_target "${CMAKE_CURRENT_BINARY_DIR}/rust-target")
set(SCRCTL_EXECUTABLE "${scrctl_cargo_target}/release/scrctl${CMAKE_EXECUTABLE_SUFFIX}")
set(scrctl_rust_link_args)
if(APPLE)
    list(APPEND scrctl_rust_link_args
        -C "link-arg=-Wl,-headerpad_max_install_names"
        -C "link-arg=-Wl,-rpath,$<TARGET_LINKER_FILE_DIR:xdb_client>")
elseif(UNIX)
    list(APPEND scrctl_rust_link_args -C "link-arg=-Wl,-rpath,$ORIGIN/lib")
endif()
add_custom_target(scrctl-rust ALL
    COMMAND "${CMAKE_COMMAND}" -E env
        "XDB_LIB_DIR=$<TARGET_LINKER_FILE_DIR:xdb_client>"
        "${SCRCTL_CARGO}" rustc --manifest-path "${PROJECT_SOURCE_DIR}/core/Cargo.toml"
        --package scrctl-cli --bin scrctl --features sdl3,ffmpeg-next --locked --release
        --target-dir "${scrctl_cargo_target}" -- ${scrctl_rust_link_args}
    BYPRODUCTS "${SCRCTL_EXECUTABLE}"
    DEPENDS xdb_client
    WORKING_DIRECTORY "${PROJECT_SOURCE_DIR}"
    USES_TERMINAL
    VERBATIM)
