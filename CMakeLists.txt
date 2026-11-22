# 最低版本保持 3.20；较新 CMake 的行为通过可用性检查显式启用。
cmake_minimum_required(VERSION 3.20)
# 下载源码解包后使用当前时间，确保依赖归档更新能触发重新构建。
# 在顶层设置，作用于全部 FetchContent 声明；include() 的策略作用域不会外传。
if(POLICY CMP0135)
    cmake_policy(SET CMP0135 NEW)
endif()

project(scrctl
        VERSION 2.0.0
        DESCRIPTION "iOS screen mirroring and remote desktop control"
        LANGUAGES CXX)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "" FORCE)
endif()

# Allow normal source builds to acquire the pinned XDB client dependency.
option(XDB_FETCH_DEPENDENCIES "Download missing pinned XDB client dependencies" ON)

# Device protocols and media capture live in a separately running xdb Server.
set(XDB_BUILD_SERVER OFF CACHE BOOL "" FORCE)
set(XDB_BUILD_CLI OFF CACHE BOOL "" FORCE)
set(XDB_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(XDB_INSTALL OFF CACHE BOOL "" FORCE)
add_subdirectory(xdb)
if(WIN32)
    set_target_properties(xdb_client PROPERTIES RUNTIME_OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}")
endif()

include(cmake/RustFrontend.cmake)
if(WIN32)
    include(cmake/WindowsRuntime.cmake)
elseif(APPLE)
    include(cmake/MacOSRuntime.cmake)
else()
    install(PROGRAMS "${SCRCTL_EXECUTABLE}" DESTINATION bin RENAME scrctl)
    install(TARGETS xdb_client LIBRARY DESTINATION bin/lib)
endif()
install(FILES LICENSE THIRD_PARTY_NOTICES.md DESTINATION share/doc/scrctl)
install(FILES third_party/stb/LICENSE DESTINATION share/licenses/stb)
install(FILES docs/DDI.md docs/SOURCE_MATERIALS.md DESTINATION share/doc/scrctl)
if(WIN32)
    install(FILES docs/WINDOWS.md DESTINATION share/doc/scrctl)
elseif(APPLE)
    install(FILES docs/MACOS.md DESTINATION share/doc/scrctl)
endif()


include(cmake/SourceMaterials.cmake)

install(FILES xdb/LICENSE xdb/COPYING.GPLv3 DESTINATION share/licenses/XDeviceBridge)
