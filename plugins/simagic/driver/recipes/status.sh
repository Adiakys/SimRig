# The JSON line a driver's `status` hook prints.

json_string() {
    if [ -z "$1" ]; then
        printf 'null'
    else
        printf '"%s"' "$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g' | tr '\n\t' '  ')"
    fi
}

# status_json INSTALLED ACTIVE VERSION NEEDS_REBOOT PROVIDED_BY_SYSTEM USER_ACCESS MESSAGE
# Booleans are `true` or `false`; VERSION and MESSAGE may be empty.
status_json() {
    printf '{"installed": %s, "active": %s, "version": %s, "needs_reboot": %s, "provided_by_system": %s, "user_access": %s, "message": %s}\n' \
        "$1" "$2" "$(json_string "$3")" "$4" "$5" "$6" "$(json_string "$7")"
}
