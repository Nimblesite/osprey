BEGIN { print "static const char osprey_gpu_shader[] =" }
{
    printf "\""
    for (i = 1; i <= length($0); i++) {
        c = substr($0, i, 1)
        if (c == "\\" || c == "\"") printf "\\"
        printf "%s", c
    }
    print "\\n\""
}
END {
    print ";"
    print "static const unsigned int osprey_gpu_shader_len = sizeof osprey_gpu_shader - 1;"
}
