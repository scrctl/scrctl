# BundleUtilities 在安装阶段扫描、复制并修复 Mach-O 依赖；独立 CLI 的 bundle
# 根就是 bin，所以库放在 bin/lib，避免它的安全检查拒绝修改根目录外的文件。
install(CODE [[
    # --prefix dist-macos 会保留相对路径；BundleUtilities 的已解析依赖使用
    # 绝对路径，安全边界比较要求 executable 根也一致。安装阶段再标准化，
    # 既支持命令行覆盖 prefix，也不会改变 configure 时的安装目录选择。
    get_filename_component(CMAKE_INSTALL_PREFIX "${CMAKE_INSTALL_PREFIX}" ABSOLUTE)
    # 独立 CLI 会让 BundleUtilities 扫描整个 bin；提前拒绝共享前缀，避免
    # 用户误把 /usr/local/bin 或其他已有程序的目录当成可移动包来修复。
    set(scrctl_root "$ENV{DESTDIR}${CMAKE_INSTALL_PREFIX}")
    file(GLOB scrctl_existing_items "${scrctl_root}/bin/*")
    foreach(scrctl_item IN LISTS scrctl_existing_items)
        get_filename_component(scrctl_name "${scrctl_item}" NAME)
        if(NOT scrctl_name STREQUAL "scrctl" AND NOT scrctl_name STREQUAL "lib")
            message(FATAL_ERROR "macOS packaging requires a standalone install prefix; unrelated item: ${scrctl_item}")
        endif()
    endforeach()
    file(GLOB scrctl_existing_libs "${scrctl_root}/bin/lib/*")
    if(scrctl_existing_libs AND NOT EXISTS "${scrctl_root}/share/doc/scrctl/runtime-origins.tsv")
        message(FATAL_ERROR "Refusing to fix existing libraries without scrctl package provenance: ${scrctl_root}/bin/lib")
    endif()
]])
install(PROGRAMS "${SCRCTL_EXECUTABLE}" DESTINATION bin RENAME scrctl)
install(CODE "set(scrctl_xdb_origin \"$<TARGET_FILE:xdb_client>\")")
install(CODE "set(scrctl_xdb_name \"$<TARGET_SONAME_FILE_NAME:xdb_client>\")")
install(CODE [[
    include(BundleUtilities)
    set(BU_CHMOD_BUNDLE_ITEMS ON)
    function(gp_item_default_embedded_path_override item default_embedded_path_var)
        set(${default_embedded_path_var} "@executable_path/lib" PARENT_SCOPE)
    endfunction()

    set(scrctl_root "$ENV{DESTDIR}${CMAKE_INSTALL_PREFIX}")
    set(scrctl_exe "${scrctl_root}/bin/scrctl")
    set(scrctl_plugins "")
    file(INSTALL DESTINATION "${CMAKE_INSTALL_PREFIX}/bin/lib" TYPE FILE FILES "${scrctl_xdb_origin}" RENAME "${scrctl_xdb_name}")
    list(APPEND scrctl_plugins "${scrctl_root}/bin/lib/${scrctl_xdb_name}")
    file(MAKE_DIRECTORY "${scrctl_root}/bin/lib" "${scrctl_root}/share/doc/scrctl")
    get_bundle_keys("${scrctl_exe}" "${scrctl_plugins}" "${scrctl_root}/bin/lib" scrctl_keys)
    set(scrctl_origins "library\toriginal_path\n")
    foreach(scrctl_key IN LISTS scrctl_keys)
        if(NOT scrctl_key STREQUAL "scrctl")
            get_filename_component(scrctl_name "${${scrctl_key}_RESOLVED_EMBEDDED_ITEM}" NAME)
            set(scrctl_origin "${${scrctl_key}_RESOLVED_ITEM}")
            # BundleUtilities 为保留 symlink 链，会把 COPYFLAG=2 的来源改为
            # 包内目标。追溯目标 key 的原始来源，避免元数据误记成安装目录。
            if("${${scrctl_key}_COPYFLAG}" STREQUAL "2")
                foreach(scrctl_target_key IN LISTS scrctl_keys)
                    if("${${scrctl_target_key}_RESOLVED_EMBEDDED_ITEM}" STREQUAL scrctl_origin)
                        set(scrctl_origin "${${scrctl_target_key}_RESOLVED_ITEM}")
                        break()
                    endif()
                endforeach()
            endif()
            get_filename_component(scrctl_origin "${scrctl_origin}" REALPATH)
            string(APPEND scrctl_origins "${scrctl_name}\t${scrctl_origin}\n")
        endif()
    endforeach()
    file(WRITE "${scrctl_root}/share/doc/scrctl/runtime-origins.tsv" "${scrctl_origins}")
    clear_bundle_keys(scrctl_keys)
    # 此成熟工具统一递归拷贝库、改写 install name、删除旧 rpath，并验证依赖。
    fixup_bundle("${scrctl_exe}" "${scrctl_plugins}" "${scrctl_root}/bin/lib")

    # install_name_tool 会使原有签名失效；Apple Silicon 要求有效的本地签名。
    # 仅重新签署包内副本，不改变 Homebrew、系统安全策略或任何系统库。
    find_program(scrctl_codesign codesign REQUIRED)
    file(GLOB_RECURSE scrctl_dylibs "${scrctl_root}/bin/lib/*.dylib")
    foreach(scrctl_binary IN LISTS scrctl_dylibs scrctl_exe)
        if(NOT IS_SYMLINK "${scrctl_binary}")
            execute_process(COMMAND "${scrctl_codesign}" --force --sign - "${scrctl_binary}"
                RESULT_VARIABLE scrctl_sign_result ERROR_VARIABLE scrctl_sign_error)
            if(NOT scrctl_sign_result EQUAL 0)
                message(FATAL_ERROR "codesign failed for ${scrctl_binary}: ${scrctl_sign_error}")
            endif()
        endif()
    endforeach()
]])
install(DIRECTORY "${PROJECT_SOURCE_DIR}/cmake/licenses/"
    DESTINATION share/doc/scrctl/dependency-licenses/gnu)
