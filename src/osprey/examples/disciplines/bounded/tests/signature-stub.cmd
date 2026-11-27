@echo off
rem Windows twin of signature-stub.sh: the same export signature, each line
rem echoed (">" escaped as "^>"); the checker reads its CRLF ends as LF.
echo signature Rate
echo     scale : (int, int) -^> int
echo     relay : (int) -^> int
echo     literals : () -^> int
echo     add : (int, int) -^> int
echo     inc : (int) -^> int
echo     ok : () -^> int
