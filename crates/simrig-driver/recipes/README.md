# Driver recipes

Ready-made pieces for drivers' hooks (architecture §6.2), as POSIX shell libraries. A driver
copies the ones it needs into its own `recipes/` directory and sources them from its hooks:
drivers must keep working, and be removable, after the app is gone, so they never depend on
files outside their own directory. A test checks that the copies match these originals.

| Recipe | Functions |
|---|---|
| `dkms.sh` | `dkms_install`, `dkms_remove`, `dkms_versions`, `dkms_installed`, `module_load` |
| `secureboot.sh` | `sb_enabled`, `sb_key`, `sb_key_enrolled`, `sb_request_enrollment` |
| `udev.sh` | `udev_install`, `udev_remove`, `udev_apply` |
| `prereq.sh` | `missing_commands`, `kernel_headers_present` |
| `status.sh` | `status_json`, the line a `status` hook prints |

They run as root from `install` and `uninstall` hooks (the helper gives them a clean
environment), except `status.sh` and the read-only checks, which also work as the user. Tests
redirect the system paths with `SIMRIG_TEST_ROOT`, which the helper never sets.
