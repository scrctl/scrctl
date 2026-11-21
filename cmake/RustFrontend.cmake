option(SCRCTL_BUILD_RUST_FRONTEND "Build the Rust CLI for installation" OFF)
set(SCRCTL_RUST_FRONTEND "" CACHE FILEPATH "Prebuilt native Rust scrctl executable")

if(SCRCTL_BUILD_RUST_FRONTEND)
    if(SCRCTL_RUST_FRONTEND)
        message(FATAL_ERROR "Choose SCRCTL_BUILD_RUST_FRONTEND or SCRCTL_RUST_FRONTEND")
    endif()
    find_program(SCRCTL_CARGO cargo REQUIRED)
    set(scrctl_cargo_target "${CMAKE_CURRENT_BINARY_DIR}/rust-target")
    set(SCRCTL_RUST_FRONTEND "${scrctl_cargo_target}/release/scrctl${CMAKE_EXECUTABLE_SUFFIX}")
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
        BYPRODUCTS "${SCRCTL_RUST_FRONTEND}"
        DEPENDS xdb_client
        WORKING_DIRECTORY "${PROJECT_SOURCE_DIR}"
        USES_TERMINAL
        VERBATIM)
elseif(SCRCTL_RUST_FRONTEND AND NOT EXISTS "${SCRCTL_RUST_FRONTEND}")
    message(FATAL_ERROR "SCRCTL_RUST_FRONTEND does not exist: ${SCRCTL_RUST_FRONTEND}")
endif()
