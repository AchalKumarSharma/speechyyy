# Speechyyy Release Runbook

This document defines the official release process for Speechyyy. Follow these procedures to build, verify, sign, test, and publish releases.

---

## 1. Asset & Archive Specifications

Speechyyy standardizes on a unified asset and archive scheme across all release consumers (`install.sh`, `speechyyy update`, and GitHub Releases):

- **Archive file name:** `speechyyy-<tag>-<arch>-linux.tar.gz`
  - `<tag>`: Matches `^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$` (e.g. `v0.2.0`, `v0.2.0-rc1`).
  - `<arch>`: System architecture matching `uname -m` (`x86_64` or `aarch64`).
- **Archive directory structure:** Exactly one top-level directory named `speechyyy-<tag>-<arch>-linux/` containing:
  - `speechyyy` (executable, mode `0755`)
  - `speechyyy-dock` (executable, mode `0755`)
  - `libsherpa-onnx-c-api.so` (and any companion `.so` runtime libraries)
  - `LICENSE`
  - `THIRD_PARTY_MODELS.md`
  - `README.md`
- **Single Checksum File:** Exactly one `SHA256SUMS` file containing the 64-character hex checksums and relative filenames of all released archives.
- **Archive Determinism:** Tarballs are packaged with fixed ownership and timestamps:
  ```bash
  tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@${SOURCE_DATE_EPOCH}" -czf "$ARCHIVE_NAME" "$STAGE_DIR"
  ```

---

## 2. CI Architecture & Security Guarantees

The GitHub Actions release pipeline is defined in [`.github/workflows/release.yml`](.github/workflows/release.yml):

- **Trigger:** Tag pushes matching `v*` and manual triggers via `workflow_dispatch`.
- **Top-Level Permissions:** Strictly `contents: read`. Elevated write permissions (`contents: write`) are isolated solely to the final `publish` job.
- **Pinned Action SHAs:** All third-party actions are pinned to full 40-character commit hashes with release tag comments:
  - `actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2`
  - `actions/upload-artifact@6f51ac03b9356f520e9adb1b1b7802705f340c2b # v4.5.0`
  - `actions/download-artifact@fa0a91b85d4f404e444e00e005971372dc801d16 # v4.1.8`
- **Native Matrix Compilation:**
  - `x86_64`: Hosted on `ubuntu-22.04` (provides broad glibc backward-compatibility, baseline glibc 2.35).
  - `aarch64`: Hosted on `ubuntu-24.04-arm` (GitHub-hosted native Arm64 runner, eliminating slow QEMU cross-emulation).
- **Draft Releases:** The workflow publishes releases in **Draft** state (`--draft`). Maintainers review and test before promoting them to public. Pre-release tags containing a hyphen (e.g. `-rc1`) automatically receive the `--prerelease` flag.

---

## 3. Pre-Flight Checklist

Before cutting a release or pre-release candidate:

1. **Verify Working Tree:** Ensure git working tree is clean on the release branch:
   ```bash
   git status
   ```
2. **Verify Manifest Versions:** Ensure `Cargo.toml` matches the planned release base version:
   ```bash
   grep -m1 '^version =' Cargo.toml
   ```
   *Note:* The pipeline automatically verifies that `TAG` base version (`vX.Y.Z`) matches `Cargo.toml`.
3. **Run Quality Gates Locally:**
   ```bash
   cargo test --all-targets --locked
   cargo clippy --all-targets --all-features --locked -- -D warnings
   ```
4. **Inspect Documentation:** Verify that `README.md`, `THIRD_PARTY_MODELS.md`, and `LICENSE` reflect the release state.

---

## 4. Step-by-Step Release Procedure

### Phase A: Dry Run with `workflow_dispatch` (Optional)

To test build artifacts and verify runner behavior without pushing a git tag:

1. Trigger via GitHub CLI:
   ```bash
   gh workflow run release.yml -f tag=v0.2.0-rc1
   ```
2. Or trigger via the GitHub web UI: **Actions** -> **Release** -> **Run workflow** -> Enter Tag `v0.2.0-rc1`.
3. Verify that all jobs (`sanity`, `test`, `build`, `publish`) succeed.
4. Download the generated draft release assets to inspect layout and permissions.

### Phase B: Tag & Publish Release Candidate (RC)

1. Create and push an annotated release candidate tag:
   ```bash
   git tag -a v0.2.0-rc1 -m "Release v0.2.0-rc1"
   git push origin v0.2.0-rc1
   ```
2. GitHub Actions will trigger `release.yml`:
   - Validates `v0.2.0-rc1` against semver format and `Cargo.toml`.
   - Runs full test suite and clippy linting.
   - Builds native `x86_64` and `aarch64` release binaries and companion libraries.
   - Packages archives and uploads a single unified `SHA256SUMS`.
   - Creates a **Draft Pre-release** on GitHub.

### Phase C: Clean Machine Verification

Test the release candidate on a fresh, non-developer system or container:

1. **Test Fresh Install via `install.sh`:**
   ```bash
   SPEECHYYY_VERSION=v0.2.0-rc1 bash <(curl -fsSL https://raw.githubusercontent.com/AchalKumarSharma/speechyyy/v0.2.0-rc1/install.sh)
   ```
   Verify that:
   - `speechyyy` and `speechyyy-dock` are placed in `~/.local/bin/`.
   - `speechyyy doctor` runs and verifies components.
   - Dynamic libraries (`libsherpa-onnx-c-api.so`) resolve without linker errors.
2. **Test In-Place Upgrade via CLI:**
   From an earlier installation, run:
   ```bash
   speechyyy update --tag v0.2.0-rc1
   ```
   Verify that:
   - Explicit tag bypasses "already up to date" check.
   - Rollback safeguards and atomic replacement succeed without process locking (`ETXTBSY`).

### Phase D: Final General Availability (GA) Release

Once the release candidate is approved:

1. Create and push the final GA tag:
   ```bash
   git tag -a v0.2.0 -m "Release v0.2.0"
   git push origin v0.2.0
   ```
2. Allow CI to finish building and publishing the draft release.
3. Review release notes in GitHub Releases.
4. Promote from Draft to Public:
   ```bash
   gh release edit v0.2.0 --draft=false
   ```
   Or click **Publish release** in the GitHub Releases web interface.

---

## 5. Rollback & Mistake Recovery

If a broken build, regression, or mistaken release occurs:

### Scenario 1: Bad Tag Pushed, Draft Release Not Published
1. Delete the local and remote tags:
   ```bash
   git tag -d v0.2.0-rc1
   git push --delete origin v0.2.0-rc1
   ```
2. Delete any orphaned draft release:
   ```bash
   gh release delete v0.2.0-rc1 --yes
   ```

### Scenario 2: Public Release Already Published with Critical Defect
Do **NOT** re-tag or mutate existing tags; users and package managers rely on immutable hashes.
1. Immediately flag or mark the release as a pre-release or edit the title with `[RECALL / DEFECTIVE]`.
2. Push a patch release tag (e.g. `v0.2.1` or `v0.2.0-patch1`).
3. If necessary, unpublish the defective release:
   ```bash
   gh release delete v0.2.0 --yes
   ```

---

## 6. Action SHA Pinning & Upgrades

For supply-chain security, GitHub Actions should not rely on floating tags (`@v4`). To audit or bump pinned actions:

1. Resolve the latest commit hash for the target action tag using GitHub CLI:
   ```bash
   gh api repos/actions/checkout/commits/v4.2.2 --jq '.sha'
   # Output: 11bd71901bbe5b1630ceea73d27597364c9af683
   ```
2. Update `.github/workflows/release.yml` with the full SHA and version comment:
   ```yaml
   uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
   ```

