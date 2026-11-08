use speechyyy::config::Config;
use speechyyy::doctor::{run_doctor_opts, CheckStatus, RealSysEnv};
use speechyyy::model::{
    compute_available_disk_space, compute_required_disk_space, download_and_install_model,
    recover_interrupted_install, safe_extract_tar_bz2, validate_download_url, Fetcher,
    InstalledModelInfo, LocalModelType, ModelManifestEntry, ModelsManifest,
};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

struct TempTestDir {
    path: PathBuf,
}

impl TempTestDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let count = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "speechyyy-test-{}-{}-{}",
            prefix,
            std::process::id(),
            count
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        TempTestDir { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempTestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn compute_sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

fn create_tar_bz2_archive(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
    {
        let mut tar = tar::Builder::new(&mut bz);
        for &(rel_path, data, is_dir) in entries {
            let mut header = tar::Header::new_gnu();
            if is_dir {
                header.set_entry_type(tar::EntryType::Directory);
                header.set_size(0);
                header.set_mode(0o755);
            } else {
                header.set_entry_type(tar::EntryType::Regular);
                header.set_size(data.len() as u64);
                header.set_mode(0o644);
            }
            header.set_cksum();
            if is_dir {
                tar.append_data(&mut header, rel_path, std::io::empty())
                    .expect("append dir");
            } else {
                tar.append_data(&mut header, rel_path, data)
                    .expect("append file");
            }
        }
        tar.finish().expect("finish tar");
    }
    bz.finish().expect("finish bz2")
}

struct MockFetcher {
    bytes: Vec<u8>,
    custom_hash: Option<String>,
}

impl Fetcher for MockFetcher {
    fn fetch(
        &self,
        _url: &str,
        dest: &Path,
        max_bytes: u64,
        progress_cb: Option<&mut dyn FnMut(u64)>,
    ) -> Result<String, String> {
        if self.bytes.len() as u64 > max_bytes {
            return Err(format!(
                "Download aborted: received {} bytes, exceeding allowed maximum of {} bytes",
                self.bytes.len(),
                max_bytes
            ));
        }

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(dest, &self.bytes).map_err(|e| e.to_string())?;

        if let Some(cb) = progress_cb {
            cb(self.bytes.len() as u64);
        }

        if let Some(ref h) = self.custom_hash {
            Ok(h.clone())
        } else {
            Ok(compute_sha256_hex(&self.bytes))
        }
    }
}

fn make_manifest(
    model_id: &str,
    sha256: &str,
    size_bytes: u64,
    max_files: usize,
    max_extracted_bytes: u64,
    expected_files: Vec<String>,
) -> ModelsManifest {
    let mut map = HashMap::new();
    map.insert(
        model_id.to_string(),
        ModelManifestEntry {
            id: model_id.to_string(),
            url: format!(
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/{}.tar.bz2",
                model_id
            ),
            sha256: sha256.to_string(),
            size_bytes,
            max_extracted_bytes,
            max_files,
            expected_files,
            license: "MIT".to_string(),
            license_url: "https://example.com/license".to_string(),
            attribution: "Test Attribution".to_string(),
        },
    );
    ModelsManifest { models: map }
}

// 1. Valid archive install: mock fetcher supplies a crafted .tar.bz2 containing all expected files
#[test]
fn test_valid_archive_install() {
    let temp = TempTestDir::new("valid-install");
    let target_dir = temp.path().join("parakeet-tdt-v3");

    let container = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
    let entries = [
        (&format!("{}/encoder.int8.onnx", container)[..], b"encoder-bytes".as_slice(), false),
        (&format!("{}/decoder.int8.onnx", container)[..], b"decoder-bytes".as_slice(), false),
        (&format!("{}/joiner.int8.onnx", container)[..], b"joiner-bytes".as_slice(), false),
        (&format!("{}/tokens.txt", container)[..], b"tokens-bytes".as_slice(), false),
    ];
    let archive_bytes = create_tar_bz2_archive(&entries);
    let sha256 = compute_sha256_hex(&archive_bytes);
    let size_bytes = archive_bytes.len() as u64;

    let manifest = make_manifest(
        "parakeet-tdt-v3",
        &sha256,
        size_bytes,
        10,
        100_000,
        vec![
            "encoder.int8.onnx".into(),
            "decoder.int8.onnx".into(),
            "joiner.int8.onnx".into(),
            "tokens.txt".into(),
        ],
    );

    let fetcher = MockFetcher {
        bytes: archive_bytes,
        custom_hash: None,
    };

    let result = download_and_install_model(
        LocalModelType::ParakeetTdtV3,
        &target_dir,
        &fetcher,
        Some(&manifest),
    );

    assert!(result.is_ok(), "Expected install success, got: {:?}", result);
    let info = result.unwrap();
    assert_eq!(info.model_type, LocalModelType::ParakeetTdtV3);
    assert_eq!(info.file_count, 4);

    // Verify all files were installed with correct contents
    assert_eq!(
        fs::read(target_dir.join("encoder.int8.onnx")).unwrap(),
        b"encoder-bytes"
    );
    assert_eq!(
        fs::read(target_dir.join("decoder.int8.onnx")).unwrap(),
        b"decoder-bytes"
    );
    assert_eq!(
        fs::read(target_dir.join("joiner.int8.onnx")).unwrap(),
        b"joiner-bytes"
    );
    assert_eq!(
        fs::read(target_dir.join("tokens.txt")).unwrap(),
        b"tokens-bytes"
    );

    // Verify MODEL_INFO.json exists and contains correct metadata
    let model_info = InstalledModelInfo::load(&target_dir).expect("load MODEL_INFO.json");
    assert_eq!(model_info.model_id, "parakeet-tdt-v3");
    assert_eq!(model_info.archive_sha256, sha256);
    assert_eq!(model_info.files.len(), 4);
    assert_eq!(
        model_info.files.get("encoder.int8.onnx").unwrap().size_bytes,
        b"encoder-bytes".len() as u64
    );

    // Verify permissions: regular file 0644
    let perms = fs::metadata(target_dir.join("encoder.int8.onnx")).unwrap().permissions();
    assert_eq!(perms.mode() & 0o777, 0o644);

    // Verify staging directory is cleaned up
    let staging_root = temp.path().join(".staging");
    if staging_root.exists() {
        let entries: Vec<_> = fs::read_dir(&staging_root).unwrap().collect();
        assert!(entries.is_empty(), "Staging dir should be empty or removed");
    }
}

// 2. Hash mismatch: mock fetcher returns valid archive but manifest has different sha256
#[test]
fn test_hash_mismatch_rejected() {
    let temp = TempTestDir::new("hash-mismatch");
    let target_dir = temp.path().join("parakeet-tdt-v3");

    let container = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
    let entries = [
        (&format!("{}/encoder.int8.onnx", container)[..], b"encoder-bytes".as_slice(), false),
        (&format!("{}/decoder.int8.onnx", container)[..], b"decoder-bytes".as_slice(), false),
        (&format!("{}/joiner.int8.onnx", container)[..], b"joiner-bytes".as_slice(), false),
        (&format!("{}/tokens.txt", container)[..], b"tokens-bytes".as_slice(), false),
    ];
    let archive_bytes = create_tar_bz2_archive(&entries);
    let size_bytes = archive_bytes.len() as u64;

    // Provide a mismatched hash
    let fake_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let manifest = make_manifest(
        "parakeet-tdt-v3",
        fake_hash,
        size_bytes,
        10,
        100_000,
        vec!["encoder.int8.onnx".into()],
    );

    let fetcher = MockFetcher {
        bytes: archive_bytes,
        custom_hash: None,
    };

    let result = download_and_install_model(
        LocalModelType::ParakeetTdtV3,
        &target_dir,
        &fetcher,
        Some(&manifest),
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("SHA-256 checksum mismatch"),
        "Expected hash mismatch error, got: {}",
        err
    );
    assert!(!target_dir.exists(), "Target dir must not be created");
}

// 3. Archive larger than size_bytes: fetcher returns a body longer than manifest's size_bytes
#[test]
fn test_archive_larger_than_size_bytes_aborted() {
    let temp = TempTestDir::new("size-abort");
    let target_dir = temp.path().join("parakeet-tdt-v3");

    let container = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
    let entries = [
        (&format!("{}/encoder.int8.onnx", container)[..], b"encoder-bytes".as_slice(), false),
        (&format!("{}/decoder.int8.onnx", container)[..], b"decoder-bytes".as_slice(), false),
        (&format!("{}/joiner.int8.onnx", container)[..], b"joiner-bytes".as_slice(), false),
        (&format!("{}/tokens.txt", container)[..], b"tokens-bytes".as_slice(), false),
    ];
    let archive_bytes = create_tar_bz2_archive(&entries);
    let sha256 = compute_sha256_hex(&archive_bytes);

    // Manifest claims size is smaller than the archive
    let small_size = (archive_bytes.len() / 2) as u64;
    let manifest = make_manifest(
        "parakeet-tdt-v3",
        &sha256,
        small_size,
        10,
        100_000,
        vec!["encoder.int8.onnx".into()],
    );

    let fetcher = MockFetcher {
        bytes: archive_bytes,
        custom_hash: None,
    };

    let result = download_and_install_model(
        LocalModelType::ParakeetTdtV3,
        &target_dir,
        &fetcher,
        Some(&manifest),
    );

    assert!(result.is_err());
    assert!(!target_dir.exists());
}

// 4. ../evil, absolute path /etc/evil, symlink, hardlink: each rejected
#[test]
fn test_path_traversal_guards() {
    let temp = TempTestDir::new("path-traversal");
    let dest_dir = temp.path().join("dest");

    // 4a: Relative path with ..
    {
        use std::io::Write;
        let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
        {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(12);
            header.set_mode(0o644);
            let path_bytes = b"container/../evil.txt";
            header.as_mut_bytes()[..path_bytes.len()].copy_from_slice(path_bytes);
            header.set_cksum();

            bz.write_all(header.as_bytes()).unwrap();
            let mut data_buf = [0u8; 512];
            data_buf[..12].copy_from_slice(b"evil-payload");
            bz.write_all(&data_buf).unwrap();
            bz.write_all(&[0u8; 1024]).unwrap();
        }
        let archive = bz.finish().unwrap();
        let archive_path = temp.path().join("traversal.tar.bz2");
        fs::write(&archive_path, archive).unwrap();

        let res = safe_extract_tar_bz2(&archive_path, &dest_dir, 10, 100_000);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("illegal path traversal component '..'"));
        assert!(!dest_dir.join("evil.txt").exists());
        assert!(!temp.path().join("evil.txt").exists());
    }

    // 4b: Absolute path
    {
        use std::io::Write;
        let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
        {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(12);
            header.set_mode(0o644);
            let path_bytes = b"/etc/evil.txt";
            header.as_mut_bytes()[..path_bytes.len()].copy_from_slice(path_bytes);
            header.set_cksum();

            bz.write_all(header.as_bytes()).unwrap();
            let mut data_buf = [0u8; 512];
            data_buf[..12].copy_from_slice(b"evil-payload");
            bz.write_all(&data_buf).unwrap();
            bz.write_all(&[0u8; 1024]).unwrap();
        }
        let archive = bz.finish().unwrap();
        let archive_path = temp.path().join("absolute.tar.bz2");
        fs::write(&archive_path, archive).unwrap();

        let res = safe_extract_tar_bz2(&archive_path, &dest_dir, 10, 100_000);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("illegal absolute path component"));
    }

    // 4c: Symlink entry
    {
        let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
        {
            let mut tar = tar::Builder::new(&mut bz);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            header.set_cksum();
            tar.append_link(&mut header, "container/link", "/etc/passwd").unwrap();
            tar.finish().unwrap();
        }
        let archive = bz.finish().unwrap();
        let archive_path = temp.path().join("symlink.tar.bz2");
        fs::write(&archive_path, archive).unwrap();

        let res = safe_extract_tar_bz2(&archive_path, &dest_dir, 10, 100_000);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("forbidden entry type"));
    }

    // 4d: Hardlink entry
    {
        let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
        {
            let mut tar = tar::Builder::new(&mut bz);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Link);
            header.set_size(0);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_link(&mut header, "container/hardlink", "container/target").unwrap();
            tar.finish().unwrap();
        }
        let archive = bz.finish().unwrap();
        let archive_path = temp.path().join("hardlink.tar.bz2");
        fs::write(&archive_path, archive).unwrap();

        let res = safe_extract_tar_bz2(&archive_path, &dest_dir, 10, 100_000);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("forbidden entry type"));
    }
}

// 5. Archive with ./ prefix (e.g. ./model-dir/tokens.txt): extracts correctly with container directory stripped
#[test]
fn test_archive_with_curdir_prefix() {
    let temp = TempTestDir::new("curdir-prefix");
    let dest_dir = temp.path().join("dest");

    let entries = [
        ("./container/tokens.txt", b"tokens-data".as_slice(), false),
        ("./container/encoder.int8.onnx", b"encoder-data".as_slice(), false),
    ];
    let archive = create_tar_bz2_archive(&entries);
    let archive_path = temp.path().join("curdir.tar.bz2");
    fs::write(&archive_path, archive).unwrap();

    let res = safe_extract_tar_bz2(&archive_path, &dest_dir, 10, 100_000);
    assert!(res.is_ok(), "Extraction should succeed, got: {:?}", res);

    assert_eq!(fs::read(dest_dir.join("tokens.txt")).unwrap(), b"tokens-data");
    assert_eq!(fs::read(dest_dir.join("encoder.int8.onnx")).unwrap(), b"encoder-data");
}

// 6. Decompression bomb: archive that decompresses beyond max_extracted_bytes
#[test]
fn test_decompression_bomb_capped() {
    let temp = TempTestDir::new("decompression-bomb");
    let dest_dir = temp.path().join("dest");

    let large_data = vec![b'A'; 2000];
    let entries = [("container/large.bin", large_data.as_slice(), false)];
    let archive = create_tar_bz2_archive(&entries);
    let archive_path = temp.path().join("bomb.tar.bz2");
    fs::write(&archive_path, archive).unwrap();

    // Cap at 1000 bytes
    let res = safe_extract_tar_bz2(&archive_path, &dest_dir, 10, 1000);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("exceeded cap of") || err.contains("exceeded byte cap"),
        "Got: {}",
        err
    );
}

// 7. Too many files: archive with more files than max_files
#[test]
fn test_too_many_files_capped() {
    let temp = TempTestDir::new("too-many-files");
    let dest_dir = temp.path().join("dest");

    let entries = [
        ("container/file1.bin", b"1".as_slice(), false),
        ("container/file2.bin", b"2".as_slice(), false),
        ("container/file3.bin", b"3".as_slice(), false),
    ];
    let archive = create_tar_bz2_archive(&entries);
    let archive_path = temp.path().join("many.tar.bz2");
    fs::write(&archive_path, archive).unwrap();

    // Cap at 2 files
    let res = safe_extract_tar_bz2(&archive_path, &dest_dir, 2, 100_000);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.contains("exceeded file count cap"), "Got: {}", err);
}

// 8. Missing expected file: verify_model_files() fails, no promotion
#[test]
fn test_missing_expected_file_fails_verification() {
    let temp = TempTestDir::new("missing-expected");
    let target_dir = temp.path().join("parakeet-tdt-v3");

    let container = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
    // Only 3 of 4 required files (missing tokens.txt)
    let entries = [
        (&format!("{}/encoder.int8.onnx", container)[..], b"enc".as_slice(), false),
        (&format!("{}/decoder.int8.onnx", container)[..], b"dec".as_slice(), false),
        (&format!("{}/joiner.int8.onnx", container)[..], b"join".as_slice(), false),
    ];
    let archive_bytes = create_tar_bz2_archive(&entries);
    let sha256 = compute_sha256_hex(&archive_bytes);
    let size_bytes = archive_bytes.len() as u64;

    let manifest = make_manifest(
        "parakeet-tdt-v3",
        &sha256,
        size_bytes,
        10,
        100_000,
        vec!["encoder.int8.onnx".into()],
    );

    let fetcher = MockFetcher {
        bytes: archive_bytes,
        custom_hash: None,
    };

    let result = download_and_install_model(
        LocalModelType::ParakeetTdtV3,
        &target_dir,
        &fetcher,
        Some(&manifest),
    );

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("failed file verification") || err.contains("Missing required model file"),
        "Got: {}",
        err
    );
    assert!(!target_dir.exists(), "Target dir should not be promoted");
}

// 9. Failed update keeps old model: existing model present, download fails at hash check
#[test]
fn test_failed_update_preserves_old_model() {
    let temp = TempTestDir::new("failed-update");
    let target_dir = temp.path().join("parakeet-tdt-v3");

    // Existing model
    fs::create_dir_all(&target_dir).unwrap();
    fs::write(target_dir.join("encoder.int8.onnx"), b"old-encoder").unwrap();
    fs::write(target_dir.join("decoder.int8.onnx"), b"old-decoder").unwrap();
    fs::write(target_dir.join("joiner.int8.onnx"), b"old-joiner").unwrap();
    fs::write(target_dir.join("tokens.txt"), b"old-tokens").unwrap();

    let container = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
    let entries = [
        (&format!("{}/encoder.int8.onnx", container)[..], b"new-encoder".as_slice(), false),
        (&format!("{}/decoder.int8.onnx", container)[..], b"new-decoder".as_slice(), false),
        (&format!("{}/joiner.int8.onnx", container)[..], b"new-joiner".as_slice(), false),
        (&format!("{}/tokens.txt", container)[..], b"new-tokens".as_slice(), false),
    ];
    let archive_bytes = create_tar_bz2_archive(&entries);
    let size_bytes = archive_bytes.len() as u64;

    // Bad hash in manifest
    let manifest = make_manifest(
        "parakeet-tdt-v3",
        "badhashbadhashbadhashbadhashbadhashbadhashbadhashbadhashbadhashb",
        size_bytes,
        10,
        100_000,
        vec!["encoder.int8.onnx".into()],
    );

    let fetcher = MockFetcher {
        bytes: archive_bytes,
        custom_hash: None,
    };

    let result = download_and_install_model(
        LocalModelType::ParakeetTdtV3,
        &target_dir,
        &fetcher,
        Some(&manifest),
    );

    assert!(result.is_err());
    // Old model files remain intact
    assert_eq!(
        fs::read(target_dir.join("encoder.int8.onnx")).unwrap(),
        b"old-encoder"
    );
    let old_backup = PathBuf::from(format!("{}.old", target_dir.display()));
    assert!(!old_backup.exists(), "No .old backup should be left behind");
}

// 10. Successful update replaces old model: old model replaced, no .old left behind
#[test]
fn test_successful_update_replaces_old_model() {
    let temp = TempTestDir::new("success-update");
    let target_dir = temp.path().join("parakeet-tdt-v3");

    // Existing model v1
    fs::create_dir_all(&target_dir).unwrap();
    fs::write(target_dir.join("encoder.int8.onnx"), b"v1-encoder").unwrap();
    fs::write(target_dir.join("decoder.int8.onnx"), b"v1-decoder").unwrap();
    fs::write(target_dir.join("joiner.int8.onnx"), b"v1-joiner").unwrap();
    fs::write(target_dir.join("tokens.txt"), b"v1-tokens").unwrap();

    let container = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
    let entries = [
        (&format!("{}/encoder.int8.onnx", container)[..], b"v2-encoder".as_slice(), false),
        (&format!("{}/decoder.int8.onnx", container)[..], b"v2-decoder".as_slice(), false),
        (&format!("{}/joiner.int8.onnx", container)[..], b"v2-joiner".as_slice(), false),
        (&format!("{}/tokens.txt", container)[..], b"v2-tokens".as_slice(), false),
    ];
    let archive_bytes = create_tar_bz2_archive(&entries);
    let sha256 = compute_sha256_hex(&archive_bytes);
    let size_bytes = archive_bytes.len() as u64;

    let manifest = make_manifest(
        "parakeet-tdt-v3",
        &sha256,
        size_bytes,
        10,
        100_000,
        vec!["encoder.int8.onnx".into()],
    );

    let fetcher = MockFetcher {
        bytes: archive_bytes,
        custom_hash: None,
    };

    let result = download_and_install_model(
        LocalModelType::ParakeetTdtV3,
        &target_dir,
        &fetcher,
        Some(&manifest),
    );

    assert!(result.is_ok());
    assert_eq!(
        fs::read(target_dir.join("encoder.int8.onnx")).unwrap(),
        b"v2-encoder"
    );
    let old_backup = PathBuf::from(format!("{}.old", target_dir.display()));
    assert!(!old_backup.exists(), "No .old backup should remain");
}

// 11. Interrupted install recovery: restores .old when model missing; removes .old when both present
#[test]
fn test_interrupted_install_recovery() {
    let temp = TempTestDir::new("recovery");
    let target_dir = temp.path().join("parakeet-tdt-v3");
    let old_dir = PathBuf::from(format!("{}.old", target_dir.display()));

    // Case A: target_dir missing, old_dir present
    fs::create_dir_all(&old_dir).unwrap();
    fs::write(old_dir.join("encoder.int8.onnx"), b"restored-content").unwrap();

    assert!(!target_dir.exists());
    assert!(old_dir.exists());

    recover_interrupted_install(&target_dir, "parakeet-tdt-v3").unwrap();

    assert!(target_dir.exists(), "target_dir should be restored from .old");
    assert!(!old_dir.exists(), ".old should no longer exist");
    assert_eq!(
        fs::read(target_dir.join("encoder.int8.onnx")).unwrap(),
        b"restored-content"
    );

    // Case B: both target_dir and old_dir present
    fs::create_dir_all(&old_dir).unwrap();
    fs::write(old_dir.join("dangling.bin"), b"dangling").unwrap();

    recover_interrupted_install(&target_dir, "parakeet-tdt-v3").unwrap();
    assert!(target_dir.exists());
    assert!(!old_dir.exists(), "Dangling .old should be removed when target exists");
}

// 12. Empty or placeholder sha256: refused (fail-closed)
#[test]
fn test_empty_or_placeholder_sha256_refused() {
    let manifest_empty = make_manifest(
        "parakeet-tdt-v3",
        "",
        1000,
        10,
        100_000,
        vec!["encoder.int8.onnx".into()],
    );
    let entry_empty = manifest_empty.get("parakeet-tdt-v3").unwrap();
    let res_empty = entry_empty.validate_for_download();
    assert!(res_empty.is_err());
    let err = res_empty.unwrap_err();
    assert!(err.contains("does not have a pinned checksum in this build"));
    assert!(err.contains("scripts/update-model-manifest.sh"));

    let manifest_todo = make_manifest(
        "parakeet-tdt-v3",
        "TODO",
        1000,
        10,
        100_000,
        vec!["encoder.int8.onnx".into()],
    );
    let entry_todo = manifest_todo.get("parakeet-tdt-v3").unwrap();
    let res_todo = entry_todo.validate_for_download();
    assert!(res_todo.is_err());
    assert!(res_todo.unwrap_err().contains("does not have a pinned checksum in this build"));
}

// 13. URL policy function test: accepts allowlisted https URLs, rejects bad schemes and spoof domains
#[test]
fn test_url_policy() {
    // Valid GitHub release and raw asset URLs
    assert!(validate_download_url(
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/model.tar.bz2"
    )
    .is_ok());
    assert!(validate_download_url(
        "https://objects.githubusercontent.com/github-production-release-asset-2e65be/model.tar.bz2"
    )
    .is_ok());
    assert!(validate_download_url(
        "https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/master/tokens.txt"
    )
    .is_ok());

    // Insecure HTTP
    assert!(validate_download_url(
        "http://github.com/k2-fsa/sherpa-onnx/releases/download/model.tar.bz2"
    )
    .is_err());

    // Spoofed domain suffixes
    assert!(validate_download_url(
        "https://github.com.evil.com/k2-fsa/sherpa-onnx/releases/download/model.tar.bz2"
    )
    .is_err());
    assert!(validate_download_url(
        "https://evilgithubusercontent.com/k2-fsa/sherpa-onnx/model.tar.bz2"
    )
    .is_err());

    // Userinfo spoofing attempts
    assert!(validate_download_url(
        "https://user@github.com@evil.com/model.tar.bz2"
    )
    .is_err());
    assert!(validate_download_url(
        "https://user:password@github.com/model.tar.bz2"
    )
    .is_err());
}

// 14. Disk-space check test: pure function test with synthetic statvfs values
#[test]
fn test_disk_space_available_calculation() {
    let frsize = 4096u64;
    let bavail = 100_000u64; // unprivileged available blocks
    let bfree = 200_000u64;  // total free blocks (including root reserved)

    let computed_avail = compute_available_disk_space(bavail, frsize);
    assert_eq!(computed_avail, 409_600_000); // 100_000 * 4096

    // Confirm that bavail is strictly used rather than bfree
    assert_ne!(computed_avail, bfree * frsize);

    let required = compute_required_disk_space(100_000_000, 200_000_000);
    // (archive * 2.5) + max_extracted = 250_000_000 + 200_000_000 = 450_000_000
    assert_eq!(required, 450_000_000);
}

// 15. speechyyy doctor tests: valid model, truncated file, flipped byte (--deep), dangling .old, legacy install
#[test]
fn test_doctor_integrity_checks() {
    let temp = TempTestDir::new("doctor-integrity");
    let models_dir = temp.path().join("models");
    fs::create_dir_all(&models_dir).unwrap();

    let config = Config::default();
    let env = RealSysEnv;

    // A: Valid model install
    let parakeet_dir = models_dir.join("parakeet-tdt-v3");
    fs::create_dir_all(&parakeet_dir).unwrap();
    fs::write(parakeet_dir.join("encoder.int8.onnx"), b"valid-encoder").unwrap();
    fs::write(parakeet_dir.join("decoder.int8.onnx"), b"valid-decoder").unwrap();
    fs::write(parakeet_dir.join("joiner.int8.onnx"), b"valid-joiner").unwrap();
    fs::write(parakeet_dir.join("tokens.txt"), b"valid-tokens").unwrap();

    let mut files_map = HashMap::new();
    files_map.insert(
        "encoder.int8.onnx".into(),
        speechyyy::model::FileChecksumInfo {
            sha256: compute_sha256_hex(b"valid-encoder"),
            size_bytes: b"valid-encoder".len() as u64,
        },
    );
    files_map.insert(
        "decoder.int8.onnx".into(),
        speechyyy::model::FileChecksumInfo {
            sha256: compute_sha256_hex(b"valid-decoder"),
            size_bytes: b"valid-decoder".len() as u64,
        },
    );
    files_map.insert(
        "joiner.int8.onnx".into(),
        speechyyy::model::FileChecksumInfo {
            sha256: compute_sha256_hex(b"valid-joiner"),
            size_bytes: b"valid-joiner".len() as u64,
        },
    );
    files_map.insert(
        "tokens.txt".into(),
        speechyyy::model::FileChecksumInfo {
            sha256: compute_sha256_hex(b"valid-tokens"),
            size_bytes: b"valid-tokens".len() as u64,
        },
    );

    let info = InstalledModelInfo {
        model_id: "parakeet-tdt-v3".into(),
        source_url: "https://github.com/...".into(),
        archive_sha256: "dummy".into(),
        files: files_map,
        license: "CC-BY-4.0".into(),
        license_url: "https://...".into(),
        attribution: "Test".into(),
        installed_at: "123456789".into(),
    };
    fs::write(
        parakeet_dir.join("MODEL_INFO.json"),
        serde_json::to_string_pretty(&info).unwrap(),
    )
    .unwrap();

    let report = run_doctor_opts(&env, &config, false, Some(&models_dir));
    let model_item = report
        .items
        .iter()
        .find(|i| i.name.contains("parakeet-tdt-v3"))
        .expect("model check item");
    assert_eq!(model_item.status, CheckStatus::Pass);

    // B: Truncated file in whisper-small (size mismatch)
    let whisper_dir = models_dir.join("whisper-small");
    fs::create_dir_all(&whisper_dir).unwrap();
    fs::write(whisper_dir.join("small-encoder.int8.onnx"), b"short").unwrap();
    fs::write(whisper_dir.join("small-decoder.int8.onnx"), b"decoder").unwrap();
    fs::write(whisper_dir.join("small-tokens.txt"), b"tokens").unwrap();

    let mut w_files = HashMap::new();
    w_files.insert(
        "small-encoder.int8.onnx".into(),
        speechyyy::model::FileChecksumInfo {
            sha256: "somehash".into(),
            size_bytes: 100, // expected 100, actual is 5
        },
    );
    let w_info = InstalledModelInfo {
        model_id: "whisper-small".into(),
        source_url: "https://github.com/...".into(),
        archive_sha256: "dummy".into(),
        files: w_files,
        license: "MIT".into(),
        license_url: "https://...".into(),
        attribution: "Test".into(),
        installed_at: "123456789".into(),
    };
    fs::write(
        whisper_dir.join("MODEL_INFO.json"),
        serde_json::to_string_pretty(&w_info).unwrap(),
    )
    .unwrap();

    let report = run_doctor_opts(&env, &config, false, Some(&models_dir));
    let w_item = report
        .items
        .iter()
        .find(|i| i.name.contains("whisper-small"))
        .expect("whisper check item");
    match &w_item.status {
        CheckStatus::Fail(msg) => {
            assert!(msg.contains("corrupted") || msg.contains("bytes"));
        }
        other => panic!("Expected CheckStatus::Fail for truncated file, got: {:?}", other),
    }

    // C: Flipped byte in a file with identical length (detected by --deep)
    let moonshine_dir = models_dir.join("moonshine-base");
    fs::create_dir_all(&moonshine_dir).unwrap();
    fs::write(moonshine_dir.join("tokens.txt"), b"flip-byte-tampered").unwrap();
    let mut m_files = HashMap::new();
    m_files.insert(
        "tokens.txt".into(),
        speechyyy::model::FileChecksumInfo {
            sha256: compute_sha256_hex(b"flip-byte-original"),
            size_bytes: b"flip-byte-tampered".len() as u64, // same size!
        },
    );
    let m_info = InstalledModelInfo {
        model_id: "moonshine-base".into(),
        source_url: "https://...".into(),
        archive_sha256: "dummy".into(),
        files: m_files,
        license: "MIT".into(),
        license_url: "https://...".into(),
        attribution: "Test".into(),
        installed_at: "123".into(),
    };
    fs::write(
        moonshine_dir.join("MODEL_INFO.json"),
        serde_json::to_string_pretty(&m_info).unwrap(),
    )
    .unwrap();

    // Fast check (deep = false) should pass or not fail on sha256
    let report_fast = run_doctor_opts(&env, &config, false, Some(&models_dir));
    let m_fast = report_fast
        .items
        .iter()
        .find(|i| i.name.contains("moonshine-base"))
        .unwrap();
    assert_eq!(m_fast.status, CheckStatus::Pass);

    // Deep check (deep = true) should fail on sha256 mismatch
    let report_deep = run_doctor_opts(&env, &config, true, Some(&models_dir));
    let m_deep = report_deep
        .items
        .iter()
        .find(|i| i.name.contains("moonshine-base"))
        .unwrap();
    match &m_deep.status {
        CheckStatus::Fail(msg) => {
            assert!(msg.contains("SHA-256 mismatch") || msg.contains("mismatch"));
        }
        other => panic!("Expected Fail on deep check, got: {:?}", other),
    }

    // D: Dangling .old directory
    let old_dir = models_dir.join("parakeet-tdt-v3.old");
    fs::create_dir_all(&old_dir).unwrap();
    let report_old = run_doctor_opts(&env, &config, false, Some(&models_dir));
    let old_item = report_old
        .items
        .iter()
        .find(|i| i.name.contains("parakeet-tdt-v3.old"))
        .expect("old dir check item");
    match &old_item.status {
        CheckStatus::Warn(msg) => {
            assert!(
                msg.contains("Dangling backup") || msg.contains("dangling") || msg.contains("backup"),
                "Got: {}",
                msg
            );
        }
        other => panic!("Expected Warn for dangling .old, got: {:?}", other),
    }

    // E: Legacy install without MODEL_INFO.json (warn, not fail)
    let legacy_dir = models_dir.join("whisper-large-v3-turbo");
    fs::create_dir_all(&legacy_dir).unwrap();
    fs::write(legacy_dir.join("turbo-encoder.int8.onnx"), b"turbo-enc").unwrap();
    fs::write(legacy_dir.join("turbo-decoder.int8.onnx"), b"turbo-dec").unwrap();
    fs::write(legacy_dir.join("turbo-tokens.txt"), b"turbo-tok").unwrap();
    // No MODEL_INFO.json written!

    let report_legacy = run_doctor_opts(&env, &config, false, Some(&models_dir));
    let legacy_item = report_legacy
        .items
        .iter()
        .find(|i| i.name.contains("whisper-large-v3-turbo"))
        .expect("legacy check item");
    match &legacy_item.status {
        CheckStatus::Warn(msg) => {
            assert!(
                msg.contains("unverified legacy install") && msg.contains("speechyyy download"),
                "Got: {}",
                msg
            );
        }
        other => panic!("Expected Warn for unverified legacy install, got: {:?}", other),
    }
}
