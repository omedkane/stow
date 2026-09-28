# The Flutter tooling requires that developers have CMake 3.10 or later
# installed. You should not increase this version, as doing so will cause
# the plugin to fail to compile for some customers of the plugin.
cmake_minimum_required(VERSION 3.10)

project(stow_library VERSION 0.0.1 LANGUAGES C)

find_program(CARGO_BIN cargo)
set(RUST_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../rust")

if(CARGO_BIN)
  if(CMAKE_BUILD_TYPE MATCHES "[Rr][Ee][Ll]")
    set(CARGO_FLAG "--release")
  else()
    set(CARGO_FLAG "")
  endif()

  add_custom_target(stow_rust_backend ALL
    COMMAND ${CARGO_BIN} build ${CARGO_FLAG} --manifest-path "${RUST_DIR}/Cargo.toml"
    WORKING_DIRECTORY "${RUST_DIR}"
    COMMENT "Building Rust library automatically"
  )
endif()

add_library(stow SHARED
  "stow.c"
)

if(TARGET stow_rust_backend)
  add_dependencies(stow stow_rust_backend)
endif()

set_target_properties(stow PROPERTIES
  PUBLIC_HEADER stow.h
  OUTPUT_NAME "stow"
)

target_compile_definitions(stow PUBLIC DART_SHARED_LIB)

if (ANDROID)
  # Support Android 15 16k page size
  target_link_options(stow PRIVATE "-Wl,-z,max-page-size=16384")
endif()
