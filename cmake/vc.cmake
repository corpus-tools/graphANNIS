set(VC_PREFIX ${GLOBAL_OUTPUT_PATH}/vc)


ExternalProject_Add(
  VC

  UPDATE_COMMAND ""
  PATCH_COMMAND ""

  SOURCE_DIR "${CMAKE_SOURCE_DIR}/ext/Vc-1.3.1"
  CMAKE_ARGS -BUILD_SHARED_LIBS=OFF -DTARGET_ARCHITECTURE=skylake -DBUILD_TESTING=OFF -DCMAKE_POSITION_INDEPENDENT_CODE=True -DCMAKE_INSTALL_PREFIX=${VC_PREFIX}

  TEST_COMMAND ""
)

find_package(Vc ${Vc_FIND_VERSION} QUIET NO_MODULE PATHS ${VC_PREFIX})

include(FindPackageHandleStandardArgs)
find_package_handle_standard_args(Vc CONFIG_MODE)

set(VC_STATIC_LIBRARY "${VC_PREFIX}/lib/${CMAKE_SHARED_LIBRARY_PREFIX}Vc${CMAKE_STATIC_LIBRARY_SUFFIX}")


include_directories(SYSTEM ${Vc_INCLUDE_DIR})

