# Prerequisites of a driver: commands and kernel headers.

# Prints the commands among the arguments that are not installed, one per line.
missing_commands() {
    for c in "$@"; do
        command -v "$c" >/dev/null 2>&1 || echo "$c"
    done
}

# The headers to build modules for a kernel (default: the running one) are installed.
kernel_headers_present() {
    [ -d "${SIMRIG_TEST_ROOT:-}/lib/modules/${1:-$(uname -r)}/build" ]
}
