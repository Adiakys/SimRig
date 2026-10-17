# Secure Boot: modules built on the machine are signed by DKMS with a local key (MOK), which the
# firmware must trust. Enrolling it needs a confirmation at the next boot (MokManager).

sb_enabled() {
    mokutil --sb-state 2>/dev/null | grep -q 'SecureBoot enabled'
}

# The public key DKMS signs modules with, if the distribution set one up.
sb_key() {
    for key in /var/lib/dkms/mok.pub /var/lib/shim-signed/mok/MOK.der /etc/pki/akmods/certs/public_key.der; do
        if [ -f "${SIMRIG_TEST_ROOT:-}$key" ]; then
            echo "${SIMRIG_TEST_ROOT:-}$key"
            return 0
        fi
    done
    return 1
}

sb_key_enrolled() {
    mokutil --test-key "$1" 2>/dev/null | grep -q 'is already enrolled'
}

# sb_request_enrollment KEY: asks the firmware to enroll the key at the next boot, protected by
# a one-time password, which it prints: the user types it in MokManager after restarting.
sb_request_enrollment() {
    password=$(od -An -N4 -tu4 /dev/urandom | tr -dc '0-9' | cut -c1-8)
    hash=$(mktemp)
    mokutil --generate-hash="$password" > "$hash"
    if ! mokutil --import "$1" --hash-file "$hash" >/dev/null 2>&1; then
        rm -f "$hash"
        return 1
    fi
    rm -f "$hash"
    echo "$password"
}
