# 01: CLI Scaffold & System Doctor Command

**What to build:** A command-line interface providing `start`, `stop`, `status`, and `doctor` commands, where `speechyyy doctor` inspects the user's Linux environment for input group permissions (`input`, `uinput`) and device node accessibility (`/dev/input/`, `/dev/uinput`), reporting clear diagnostic statuses and printing the exact setup commands and udev rule needed to resolve any permission gaps.

**Blocked by:** None (can start immediately)

**Status:** resolved

- [x] CLI argument parsing for `start`, `stop`, `status`, and `doctor` subcommands.
- [x] Pre-flight diagnostic checker for `/dev/input/event*` read access and `/dev/uinput` write access.
- [x] User group membership check for `input` and `uinput` groups.
- [x] `speechyyy doctor` outputs human-readable status for each check and prints the recommended `/etc/udev/rules.d/99-speechyyy.rules` and `usermod` commands when issues are detected.
- [x] Comprehensive unit tests verifying diagnostic detection logic with simulated environment states.
