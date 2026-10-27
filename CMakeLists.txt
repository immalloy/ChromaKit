cmake_minimum_required(VERSION 3.20)

project(Chromakit VERSION 0.1.0 LANGUAGES CXX)

if(APPLE)
  enable_language(OBJC)
endif()

if(POLICY CMP0169)
  cmake_policy(SET CMP0169 OLD)
endif()

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

include(FetchContent)

FetchContent_Declare(
  FLTK
  GIT_REPOSITORY https://github.com/fltk/fltk.git
  GIT_TAG release-1.4.5
  GIT_SHALLOW TRUE
)
set(FLTK_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(FLTK_BUILD_TEST OFF CACHE BOOL "" FORCE)
set(FLTK_BUILD_FLUID OFF CACHE BOOL "" FORCE)
set(FLTK_BUILD_FLTK_OPTIONS OFF CACHE BOOL "" FORCE)
set(FLTK_BUILD_FORMS OFF CACHE BOOL "" FORCE)
set(FLTK_BUILD_GL OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(FLTK)

FetchContent_Declare(
  NativeFileDialog
  GIT_REPOSITORY https://github.com/btzy/nativefiledialog-extended.git
  GIT_TAG v1.4.1
  GIT_SHALLOW TRUE
)
set(NFD_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(NFD_INSTALL OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(NativeFileDialog)

FetchContent_Declare(
  RubberBand
  GIT_REPOSITORY https://github.com/breakfastquay/rubberband.git
  GIT_TAG e4296ac80b1170018a110bc326fd0d45a0eb27d6
  GIT_SHALLOW TRUE
)
FetchContent_GetProperties(RubberBand)
if(NOT rubberband_POPULATED)
  FetchContent_Populate(RubberBand)
endif()

set(RUBBERBAND_SINGLE_CPP "${rubberband_SOURCE_DIR}/single/RubberBandSingle.cpp")

add_library(chromakit_audio STATIC
  src/audio/wav.cpp
  src/audio/pitch_detector.cpp
  src/audio/pitch_shifter.cpp
  src/audio/chromatic_generator.cpp
  ${RUBBERBAND_SINGLE_CPP}
)

target_include_directories(chromakit_audio PUBLIC
  src
  "${rubberband_SOURCE_DIR}"
)
target_compile_definitions(chromakit_audio PRIVATE NOMINMAX)

if(MSVC)
  target_compile_options(chromakit_audio PRIVATE /W4 /permissive-)
else()
  target_compile_options(chromakit_audio PRIVATE -Wall -Wextra -Wpedantic)
endif()

add_executable(chromakit WIN32 src/main.cpp)
if(WIN32)
  target_sources(chromakit PRIVATE resources/chromakit.rc)
endif()
target_link_libraries(chromakit PRIVATE chromakit_audio fltk::fltk nfd)
target_compile_definitions(chromakit PRIVATE NOMINMAX)
if(WIN32)
  target_link_libraries(chromakit PRIVATE advapi32 dwmapi)
endif()

