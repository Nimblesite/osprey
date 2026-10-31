/- String literals are checked as their ordinary UTF8 construction.
   Implements [PROOF-EXPORT]. -/
import Init
set_option autoImplicit false
namespace StringCases
 theorem empty_mk : String.mk [] = "" := rfl
 theorem unicode_mk : String.mk ['A', '😀'] = "A😀" := rfl
end StringCases
