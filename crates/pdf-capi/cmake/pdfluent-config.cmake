# pdfluent-config.cmake
#
# CMake package configuration for the PDFluent C API.
#
# Usage in a downstream project:
#
#   find_package(pdfluent REQUIRED)
#   target_link_libraries(my_target PRIVATE pdfluent::pdfluent)
#
# Variables set by this module:
#   pdfluent_FOUND         — TRUE if the library was located
#   pdfluent_VERSION       — version string (e.g. "1.0.0-beta.1")
#   pdfluent_INCLUDE_DIRS  — path(s) containing pdfluent.h
#   pdfluent_LIBRARIES     — full path(s) to the library file(s)
#
# Imported targets:
#   pdfluent::pdfluent     — IMPORTED SHARED or STATIC target
#
# Install layout expected by this config:
#   <prefix>/include/pdfluent.h            (or include/pdfluent/pdfluent.h)
#   <prefix>/lib/libpdf_capi.{so,dylib}    (or .a for static)
#   <prefix>/lib/cmake/pdfluent/pdfluent-config.cmake  (this file)

cmake_minimum_required(VERSION 3.15)

set(pdfluent_VERSION "1.0.0-beta.1")

# ----------------------------------------------------------------------------
# Locate include directory
# ----------------------------------------------------------------------------
find_path(pdfluent_INCLUDE_DIR
    NAMES pdfluent.h
    HINTS
        "${CMAKE_CURRENT_LIST_DIR}/../../../include"
        "${CMAKE_CURRENT_LIST_DIR}/../include"
        "${CMAKE_CURRENT_LIST_DIR}/../../.."
    PATH_SUFFIXES pdfluent include
)

# ----------------------------------------------------------------------------
# Locate library
# ----------------------------------------------------------------------------
find_library(pdfluent_LIBRARY
    NAMES pdf_capi
    HINTS
        "${CMAKE_CURRENT_LIST_DIR}/../../../../target/release"
        "${CMAKE_CURRENT_LIST_DIR}/../../../lib"
    PATH_SUFFIXES lib
)

# ----------------------------------------------------------------------------
# Standard result variables
# ----------------------------------------------------------------------------
include(FindPackageHandleStandardArgs)
find_package_handle_standard_args(pdfluent
    REQUIRED_VARS pdfluent_LIBRARY pdfluent_INCLUDE_DIR
    VERSION_VAR   pdfluent_VERSION
)

if(pdfluent_FOUND)
    set(pdfluent_LIBRARIES    "${pdfluent_LIBRARY}")
    set(pdfluent_INCLUDE_DIRS "${pdfluent_INCLUDE_DIR}")

    # ----------------------------------------------------------------------------
    # Imported target: pdfluent::pdfluent
    # ----------------------------------------------------------------------------
    if(NOT TARGET pdfluent::pdfluent)
        # Detect shared vs static by file extension.
        get_filename_component(_pdf_ext "${pdfluent_LIBRARY}" EXT)
        if(_pdf_ext MATCHES "\\.(so|dylib|dll)$")
            set(_pdf_link_type SHARED)
        else()
            set(_pdf_link_type STATIC)
        endif()

        add_library(pdfluent::pdfluent ${_pdf_link_type} IMPORTED)
        set_target_properties(pdfluent::pdfluent PROPERTIES
            IMPORTED_LOCATION             "${pdfluent_LIBRARY}"
            INTERFACE_INCLUDE_DIRECTORIES "${pdfluent_INCLUDE_DIR}"
        )

        # Platform link requirements for Rust cdylib
        if(APPLE)
            set_property(TARGET pdfluent::pdfluent APPEND PROPERTY
                INTERFACE_LINK_LIBRARIES
                    "-framework Security"
                    "-framework CoreFoundation"
            )
        elseif(UNIX)
            set_property(TARGET pdfluent::pdfluent APPEND PROPERTY
                INTERFACE_LINK_LIBRARIES
                    pthread dl m
            )
        endif()
    endif()
endif()

mark_as_advanced(pdfluent_LIBRARY pdfluent_INCLUDE_DIR)
