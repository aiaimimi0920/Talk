use bzip2::write::BzEncoder;
use bzip2::Compression;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use talk_desktop::{
    default_product_local_asr_model_spec, install_model_from_reader, validate_installed_model,
    ModelSpec,
};
use tar::{Builder, Header};

fn unique_temp_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ))
}

fn archive_with_files(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut tar_bytes = Vec::new();
    {
        let mut tar = Builder::new(&mut tar_bytes);
        for (path, bytes) in files {
            let mut header = Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o600);
            header.set_cksum();
            tar.append_data(&mut header, path, *bytes)
                .expect("append model fixture");
        }
        tar.finish().expect("finish model tar");
    }
    let mut compressed = Vec::new();
    let mut encoder = BzEncoder::new(&mut compressed, Compression::best());
    encoder.write_all(&tar_bytes).expect("compress model tar");
    encoder.finish().expect("finish model bzip2");
    compressed
}

fn traversal_archive() -> Vec<u8> {
    let mut tar_bytes = {
        let mut bytes = Vec::new();
        {
            let mut tar = Builder::new(&mut bytes);
            let mut header = Header::new_gnu();
            header.set_size(6);
            header.set_mode(0o600);
            header.set_cksum();
            tar.append_data(&mut header, "fixture/tokens.txt", &b"tokens"[..])
                .expect("append traversal model fixture");
            tar.finish().expect("finish traversal tar");
        }
        bytes
    };
    let header = &mut tar_bytes[..512];
    header[..100].fill(0);
    header[..15].copy_from_slice(b"../escape.txt\0\0");
    header[148..156].fill(b' ');
    let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
    let checksum_field = format!("{checksum:06o}\0 ");
    header[148..156].copy_from_slice(checksum_field.as_bytes());

    let mut compressed = Vec::new();
    let mut encoder = BzEncoder::new(&mut compressed, Compression::best());
    encoder
        .write_all(&tar_bytes)
        .expect("compress traversal tar");
    encoder.finish().expect("finish traversal bzip2");
    compressed
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn fixture_spec(archive: &[u8]) -> ModelSpec {
    ModelSpec {
        id: "fixture-model".to_string(),
        url: "https://example.invalid/fixture.tar.bz2".to_string(),
        archive_name: "fixture.tar.bz2".to_string(),
        sha256: sha256_hex(archive),
        required_files: vec![
            "tokens.txt".to_string(),
            "encoder.onnx".to_string(),
            "decoder.onnx".to_string(),
            "joiner.onnx".to_string(),
        ],
    }
}

#[test]
fn exposes_the_evidence_selected_product_local_asr_catalog_entry() {
    let spec = default_product_local_asr_model_spec();

    assert_eq!(spec.id, "zipformer-zh-en-punct-int8-480ms");
    assert_eq!(
        spec.url,
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8-2026-06-05.tar.bz2"
    );
    assert_eq!(
        spec.archive_name,
        "sherpa-onnx-x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8-2026-06-05.tar.bz2"
    );
    assert_eq!(
        spec.sha256,
        "fa5f63d618e5a01526e275a358bb7772e403f84808a4769fba52cffd8160bf74"
    );
    assert!(spec.url.starts_with("https://"));
    assert_eq!(
        spec.required_files,
        vec![
            "tokens.txt",
            "encoder.int8.onnx",
            "decoder.onnx",
            "joiner.int8.onnx",
        ]
    );
}

#[test]
fn installs_a_hash_verified_model_and_writes_a_marker() {
    let archive = archive_with_files(&[
        ("fixture/tokens.txt", b"tokens"),
        ("fixture/encoder.onnx", b"encoder"),
        ("fixture/decoder.onnx", b"decoder"),
        ("fixture/joiner.onnx", b"joiner"),
    ]);
    let spec = fixture_spec(&archive);
    let root = unique_temp_dir("talk-model-bootstrap");

    let installed = install_model_from_reader(&spec, Cursor::new(&archive), &root)
        .expect("install fixture model");

    assert_eq!(installed, root.join("fixture-model"));
    validate_installed_model(&spec, &installed).expect("validate installed model");
    assert!(installed.join("model-manifest.json").is_file());
    assert_eq!(
        std::fs::read(installed.join("tokens.txt")).expect("read tokens"),
        b"tokens"
    );

    std::fs::remove_dir_all(root).expect("remove model fixture");
}

#[test]
fn reuses_a_valid_installed_model_without_replacing_its_directory() {
    let archive = archive_with_files(&[
        ("fixture/tokens.txt", b"tokens"),
        ("fixture/encoder.onnx", b"encoder"),
        ("fixture/decoder.onnx", b"decoder"),
        ("fixture/joiner.onnx", b"joiner"),
    ]);
    let spec = fixture_spec(&archive);
    let root = unique_temp_dir("talk-model-bootstrap-reuse");
    let installed = install_model_from_reader(&spec, Cursor::new(&archive), &root)
        .expect("install fixture model");
    let sentinel = installed.join("keep-existing-install.txt");
    std::fs::write(&sentinel, b"keep").expect("write installed model sentinel");

    let reused = install_model_from_reader(&spec, Cursor::new(b"not-an-archive"), &root)
        .expect("reuse valid fixture model without reading another archive");

    assert_eq!(reused, installed);
    assert_eq!(std::fs::read(&sentinel).expect("read sentinel"), b"keep");
    validate_installed_model(&spec, &reused).expect("validate reused model");
    std::fs::remove_dir_all(root).expect("remove model fixture");
}

#[test]
fn concurrent_model_installations_converge_on_one_valid_destination() {
    const WORKERS: usize = 8;
    let archive = Arc::new(archive_with_files(&[
        ("fixture/tokens.txt", b"tokens"),
        ("fixture/encoder.onnx", b"encoder"),
        ("fixture/decoder.onnx", b"decoder"),
        ("fixture/joiner.onnx", b"joiner"),
    ]));
    let spec = Arc::new(fixture_spec(&archive));
    let root = Arc::new(unique_temp_dir("talk-model-bootstrap-concurrent"));
    let barrier = Arc::new(Barrier::new(WORKERS));
    let workers = (0..WORKERS)
        .map(|_| {
            let archive = Arc::clone(&archive);
            let spec = Arc::clone(&spec);
            let root = Arc::clone(&root);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                install_model_from_reader(&spec, Cursor::new(archive.as_slice()), &root)
            })
        })
        .collect::<Vec<_>>();

    for worker in workers {
        let installed = worker
            .join()
            .expect("model installation worker must not panic")
            .expect("concurrent model installation must succeed");
        assert_eq!(installed, root.join("fixture-model"));
    }
    validate_installed_model(&spec, &root.join("fixture-model"))
        .expect("validate concurrently installed model");
    let leftovers = std::fs::read_dir(root.as_path())
        .expect("list model root")
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with(".fixture-model."))
        .collect::<Vec<_>>();
    assert!(
        leftovers.is_empty(),
        "leftover model staging paths: {leftovers:?}"
    );
    std::fs::remove_dir_all(root.as_path()).expect("remove model fixture");
}

#[test]
fn installs_required_model_files_from_nested_directories() {
    let archive = archive_with_files(&[
        ("fixture/config/tokens.txt", b"tokens"),
        ("fixture/models/encoder.onnx", b"encoder"),
        ("fixture/models/decoder.onnx", b"decoder"),
        ("fixture/models/transducer/joiner.onnx", b"joiner"),
        ("fixture/metadata/notes.txt", b"not required"),
    ]);
    let spec = fixture_spec(&archive);
    let root = unique_temp_dir("talk-model-bootstrap-nested");

    let installed = install_model_from_reader(&spec, Cursor::new(&archive), &root)
        .expect("install nested fixture model");

    validate_installed_model(&spec, &installed).expect("validate nested model");
    assert!(installed.join("config/tokens.txt").is_file());
    assert!(installed.join("models/transducer/joiner.onnx").is_file());
    std::fs::remove_dir_all(root).expect("remove nested model fixture");
}

#[test]
fn rejects_an_archive_with_the_wrong_sha256() {
    let archive = archive_with_files(&[("fixture/tokens.txt", b"tokens")]);
    let mut spec = fixture_spec(&archive);
    spec.sha256 = "0".repeat(64);
    let root = unique_temp_dir("talk-model-bootstrap-bad-hash");

    let error = install_model_from_reader(&spec, Cursor::new(&archive), &root)
        .expect_err("wrong archive hash must fail");

    assert!(error.contains("SHA-256"), "unexpected error: {error}");
    assert!(!root.join("fixture-model").exists());
}

#[test]
fn rejects_model_catalog_path_components_that_can_escape_the_model_root() {
    let archive = archive_with_files(&[("fixture/tokens.txt", b"tokens")]);
    let root = unique_temp_dir("talk-model-bootstrap-catalog-traversal");

    for (id, archive_name) in [
        ("../escape", "fixture.tar.bz2"),
        ("fixture-model", "../escape.tar.bz2"),
        ("nested/model", "fixture.tar.bz2"),
    ] {
        let mut spec = fixture_spec(&archive);
        spec.id = id.to_string();
        spec.archive_name = archive_name.to_string();
        let error = install_model_from_reader(&spec, Cursor::new(&archive), &root)
            .expect_err("unsafe catalog path component must fail");
        assert!(
            error.contains("safe path component"),
            "unexpected error: {error}"
        );
    }
    assert!(!root.join("escape").exists());
}

#[test]
fn rejects_an_archive_with_path_traversal() {
    let archive = traversal_archive();
    let spec = fixture_spec(&archive);
    let root = unique_temp_dir("talk-model-bootstrap-traversal");

    let error = install_model_from_reader(&spec, Cursor::new(&archive), &root)
        .expect_err("traversal archive must fail");

    assert!(error.contains("relative"), "unexpected error: {error}");
    assert!(!root.join("escape.txt").exists());
}

#[test]
fn rejects_an_installed_model_missing_a_required_file() {
    let root = unique_temp_dir("talk-model-bootstrap-missing-file");
    let model_dir = root.join("fixture-model");
    std::fs::create_dir_all(&model_dir).expect("create model fixture");
    for file in ["tokens.txt", "encoder.onnx", "decoder.onnx"] {
        std::fs::write(model_dir.join(file), file.as_bytes()).expect("write model fixture");
    }
    let spec = ModelSpec {
        id: "fixture-model".to_string(),
        url: "https://example.invalid/fixture.tar.bz2".to_string(),
        archive_name: "fixture.tar.bz2".to_string(),
        sha256: "0".repeat(64),
        required_files: vec![
            "tokens.txt".to_string(),
            "encoder.onnx".to_string(),
            "decoder.onnx".to_string(),
            "joiner.onnx".to_string(),
        ],
    };

    let error = validate_installed_model(&spec, &model_dir).expect_err("missing joiner must fail");

    assert!(error.contains("joiner.onnx"), "unexpected error: {error}");
    std::fs::remove_dir_all(root).expect("remove model fixture");
}

#[allow(dead_code)]
fn read_all(mut reader: impl Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).expect("read fixture");
    bytes
}

#[allow(dead_code)]
fn assert_relative(path: &Path) {
    assert!(path.is_relative());
}
