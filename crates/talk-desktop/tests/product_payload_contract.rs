use std::fs;
use std::path::{Path, PathBuf};
use talk_desktop::{
    build_embedded_runtime_payload, extract_embedded_runtime_payload,
    parse_embedded_runtime_payload, EmbeddedRuntimePayloadSource,
};

const BASE_EXE: &[u8] = b"MZ-talk-desktop-test";

fn runtime_sources() -> Vec<EmbeddedRuntimePayloadSource<'static>> {
    vec![
        EmbeddedRuntimePayloadSource {
            path: "talk-local-asr-sherpa.exe",
            bytes: b"worker",
        },
        EmbeddedRuntimePayloadSource {
            path: "sherpa-onnx-c-api.dll",
            bytes: b"c-api",
        },
        EmbeddedRuntimePayloadSource {
            path: "sherpa-onnx-cxx-api.dll",
            bytes: b"cxx-api",
        },
        EmbeddedRuntimePayloadSource {
            path: "onnxruntime.dll",
            bytes: b"onnx-runtime",
        },
        EmbeddedRuntimePayloadSource {
            path: "onnxruntime_providers_shared.dll",
            bytes: b"onnx-provider",
        },
    ]
}

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

#[test]
fn parses_the_expected_embedded_runtime_members() {
    let executable =
        build_embedded_runtime_payload(BASE_EXE, &runtime_sources()).expect("build payload");

    let payload = parse_embedded_runtime_payload(&executable).expect("parse payload");

    assert_eq!(payload.files.len(), 5);
    assert_eq!(
        payload
            .files
            .iter()
            .map(|file| file.path.as_path())
            .collect::<Vec<_>>(),
        vec![
            Path::new("onnxruntime.dll"),
            Path::new("onnxruntime_providers_shared.dll"),
            Path::new("sherpa-onnx-c-api.dll"),
            Path::new("sherpa-onnx-cxx-api.dll"),
            Path::new("talk-local-asr-sherpa.exe"),
        ]
    );
    assert_eq!(payload.archive_sha256.len(), 64);
}

#[test]
fn rejects_an_archive_whose_bytes_no_longer_match_the_trailer_hash() {
    let mut executable =
        build_embedded_runtime_payload(BASE_EXE, &runtime_sources()).expect("build payload");
    executable[BASE_EXE.len() + 4] ^= 0x55;

    let error = parse_embedded_runtime_payload(&executable).expect_err("corrupt payload must fail");

    assert!(error.contains("SHA-256"), "unexpected error: {error}");
}

#[test]
fn rejects_payload_members_outside_the_runtime_allowlist() {
    let mut sources = runtime_sources();
    sources.push(EmbeddedRuntimePayloadSource {
        path: "asr-bench.exe",
        bytes: b"developer tool",
    });

    let error = build_embedded_runtime_payload(BASE_EXE, &sources)
        .expect_err("developer tools must not enter the product payload");

    assert!(error.contains("unexpected"), "unexpected error: {error}");
}

#[test]
fn rejects_payload_member_path_traversal() {
    let mut sources = runtime_sources();
    sources[0].path = "../talk-local-asr-sherpa.exe";

    let error = build_embedded_runtime_payload(BASE_EXE, &sources)
        .expect_err("path traversal must not enter the payload");

    assert!(error.contains("relative"), "unexpected error: {error}");
}

#[test]
fn extracts_to_a_content_addressed_runtime_directory_and_reuses_it() {
    let root = unique_temp_dir("talk-product-payload-extract");
    let executable =
        build_embedded_runtime_payload(BASE_EXE, &runtime_sources()).expect("build payload");

    let first = extract_embedded_runtime_payload(&executable, &root).expect("first extraction");
    let second = extract_embedded_runtime_payload(&executable, &root).expect("second extraction");

    assert_eq!(first, second);
    assert_eq!(
        fs::read(first.join("talk-local-asr-sherpa.exe")).expect("read worker"),
        b"worker"
    );
    assert!(first.join(".verified-runtime.json").is_file());
    assert_eq!(
        fs::read_dir(&root)
            .expect("read runtime root")
            .filter_map(Result::ok)
            .count(),
        1
    );

    fs::remove_dir_all(root).expect("remove payload fixture");
}
