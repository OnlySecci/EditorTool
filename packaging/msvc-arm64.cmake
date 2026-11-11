# FLTK 1.5.23 still defines its pre-UCRT rint fallback on Windows ARM64.
# The modern static ARM64 UCRT also exports rint, causing LNK2005. Keep the
# fallback for other targets and let ARM64 use the CRT implementation.
set(fltk_math "${CMAKE_CURRENT_SOURCE_DIR}/fltk/FL/math.h")
if(EXISTS "${fltk_math}")
  file(READ "${fltk_math}" math_header)
  string(REPLACE
    "inline double rint(double v) {return floor(v+.5);}"
    "#if !defined(_M_ARM64)\ninline double rint(double v) {return floor(v+.5);}\n#endif"
    math_header "${math_header}")
  file(WRITE "${fltk_math}" "${math_header}")
endif()
