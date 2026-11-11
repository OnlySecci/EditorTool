# Keep the C wrapper, FLTK itself and Rust on the static Microsoft runtime.
# FLTK has its own cache option; setting only CMAKE_MSVC_RUNTIME_LIBRARY in
# the parent project does not override that subproject option.
set(FLTK_MSVC_RUNTIME_DLL OFF CACHE BOOL "Use the static MSVC runtime" FORCE)
set(CFLTK_MSVC_CRT_STATIC ON CACHE BOOL "Use the static MSVC runtime" FORCE)
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>" CACHE STRING "MSVC runtime" FORCE)
if("$ENV{CARGO_BUILD_TARGET}" STREQUAL "aarch64-pc-windows-msvc")
  set(CMAKE_PROJECT_INCLUDE "${CMAKE_CURRENT_LIST_DIR}/msvc-arm64.cmake")
endif()
