@echo off
rem Windows twin of failing-stub.sh: a compiler that refuses the erased module.
>&2 echo fixture: the erased module does not infer
exit /b 1
