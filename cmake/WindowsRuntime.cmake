# 让 Windows 安装目录包含实际导入的 DLL，使用 CMake 的依赖扫描器递归解析。
# 额外目录可由 MSVC/vcpkg 等工具链显式提供；不复制 Windows 系统 DLL。
if(CMAKE_VERSION VERSION_LESS 3.21)
    message(FATAL_ERROR "Windows runtime packaging requires CMake 3.21 or newer")
endif()
get_filename_component(scrctl_compiler_bindir "${CMAKE_CXX_COMPILER}" DIRECTORY)
set(SCRCTL_RUNTIME_DLL_DIRS "${scrctl_compiler_bindir}" CACHE STRING
    "Windows DLL search directories for installation")
list(PREPEND SCRCTL_RUNTIME_DLL_DIRS "$<TARGET_FILE_DIR:xdb_client>")
add_executable(scrctl_package IMPORTED)
set_target_properties(scrctl_package PROPERTIES IMPORTED_LOCATION "${SCRCTL_EXECUTABLE}")
install(IMPORTED_RUNTIME_ARTIFACTS scrctl_package
    RUNTIME_DEPENDENCY_SET scrctl_runtime RUNTIME DESTINATION bin)

install(RUNTIME_DEPENDENCY_SET scrctl_runtime
    DIRECTORIES ${SCRCTL_RUNTIME_DLL_DIRS}
    PRE_EXCLUDE_REGEXES "api-ms-.*" "ext-ms-.*"
    POST_EXCLUDE_REGEXES "[Ww][Ii][Nn][Dd][Oo][Ww][Ss][/\\\\]"
    RUNTIME DESTINATION bin)

install(FILES $<TARGET_FILE:xdb_client> DESTINATION bin)

# 一些 MSYS2 包未安装 GNU 许可证正文，随安装目录补充；项目许可另行保留。
install(DIRECTORY "${PROJECT_SOURCE_DIR}/cmake/licenses/"
    DESTINATION share/doc/scrctl/dependency-licenses)
