use std::fs;
use std::path::{Path, PathBuf};
use talk_desktop::{
    build_embedded_runtime_payload, embedded_runtime_payload_is_appended,
    extract_embedded_runtime_payload, locate_verified_embedded_runtime,
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

#[test]
fn parser_magic_inside_an_unbundled_executable_does_not_count_as_product_payload() {
    let executable = b"MZ-talk-desktop-parser-literal-TLPAY001-without-a-payload-trailer";

    assert!(!embedded_runtime_payload_is_appended(executable));
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

    assert!(embedded_runtime_payload_is_appended(&executable));
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

#[test]
fn locates_a_verified_runtime_cache_from_the_trailer_without_reading_the_whole_executable() {
    let root = unique_temp_dir("talk-product-payload-fast-path");
    let executable_bytes =
        build_embedded_runtime_payload(BASE_EXE, &runtime_sources()).expect("build payload");
    let executable_path = root.join("Talk.exe");
    fs::create_dir_all(&root).expect("create fast path fixture root");
    fs::write(&executable_path, &executable_bytes).expect("write payload executable");
    let runtime_root = root.join("runtime");

    assert_eq!(
        locate_verified_embedded_runtime(&executable_path, &runtime_root),
        None,
        "cache miss before extraction must fall back to the full read path"
    );

    let extracted =
        extract_embedded_runtime_payload(&executable_bytes, &runtime_root).expect("extract");

    assert_eq!(
        locate_verified_embedded_runtime(&executable_path, &runtime_root),
        Some(extracted.clone())
    );

    fs::remove_dir_all(root).expect("remove fast path fixture");
}

#[test]
fn fast_path_rejects_executables_without_a_payload_and_tampered_caches() {
    let root = unique_temp_dir("talk-product-payload-fast-path-miss");
    fs::create_dir_all(&root).expect("create fast path miss fixture root");
    let runtime_root = root.join("runtime");

    let plain_executable_path = root.join("plain.exe");
    fs::write(&plain_executable_path, BASE_EXE).expect("write plain executable");
    assert_eq!(
        locate_verified_embedded_runtime(&plain_executable_path, &runtime_root),
        None,
        "an executable without an appended payload has no runtime cache"
    );

    let executable_bytes =
        build_embedded_runtime_payload(BASE_EXE, &runtime_sources()).expect("build payload");
    let executable_path = root.join("Talk.exe");
    fs::write(&executable_path, &executable_bytes).expect("write payload executable");
    let extracted =
        extract_embedded_runtime_payload(&executable_bytes, &runtime_root).expect("extract");
    fs::write(extracted.join("talk-local-asr-sherpa.exe"), b"tampered").expect("tamper worker");

    assert_eq!(
        locate_verified_embedded_runtime(&executable_path, &runtime_root),
        None,
        "a tampered cache member must force the repair path"
    );

    fs::remove_dir_all(root).expect("remove fast path miss fixture");
}

#[test]
fn replaces_a_cached_runtime_member_when_its_hash_no_longer_matches() {
    let root = unique_temp_dir("talk-product-payload-repair");
    let executable =
        build_embedded_runtime_payload(BASE_EXE, &runtime_sources()).expect("build payload");
    let destination =
        extract_embedded_runtime_payload(&executable, &root).expect("first extraction");
    let worker = destination.join("talk-local-asr-sherpa.exe");
    fs::write(&worker, b"tampered worker").expect("tamper cached worker");

    let repaired = extract_embedded_runtime_payload(&executable, &root).expect("repair extraction");

    assert_eq!(repaired, destination);
    assert_eq!(fs::read(worker).expect("read repaired worker"), b"worker");
    fs::remove_dir_all(root).expect("remove payload repair fixture");
}
