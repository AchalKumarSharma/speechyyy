# System Permissions Doctor Command

Running low-level kernel input capture and injection requires user membership in `input` and `uinput` groups or appropriate udev rules, which are absent by default on many Arch, CachyOS, and Omarchy installations. We decided to implement a dedicated `speechyyy doctor` command that inspects system group memberships and device node permissions, displaying actionable remediation steps and offering to install `/etc/udev/rules.d/99-speechyyy.rules`. This avoids opaque runtime permission errors and turns system setup into a guided, self-contained procedure.

## Considered Options

- **Requiring the daemon to run as root**: Rejected because running user-space speech daemons and network services with superuser privileges is a serious security hazard.
- **Relying solely on external documentation**: Rejected because users frequently miss setup steps and encounter cryptic `EACCES` crashes.

