use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const PAYLOAD_MAGIC: &[u8; 8] = b"TLPAY001";
const PAYLOAD_FORMAT_VERSION: u32 = 1;
const PAYLOAD_TRAILER_LEN: usize = 8 + 4 + 8 + 8 + 32;
const VERIFIED_MARKER_NAME: &str = ".verified-runtime.json";
const EXPECTED_RUNTIME_FILES: [&str; 5] = [
    "onnxruntime.dll",
    "onnxruntime_providers_shared.dll",
    "sherpa-onnx-c-api.dll",
    "sherpa-onnx-cxx-api.dll",
    "talk-local-asr-sherpa.exe",
];

#[derive(Debug, Clone, Copy)]
pub struct EmbeddedRuntimePayloadSource<'a> {
    pub path: &'a str,
    pub bytes: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedRuntimePayloadFile {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct EmbeddedRuntimePayload {
    pub archive_sha256: String,
    pub files: Vec<EmbeddedRuntimePayloadFile>,
    archive: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PayloadManifest {
    schema_version: u32,
    files: Vec<PayloadManifestFile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PayloadManifestFile {
    path: String,
    sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerifiedRuntimeMarker {
    schema_version: u32,
    archive_sha256: String,
}

pub fn build_embedded_runtime_payload(
    base_executable: &[u8],
    sources: &[EmbeddedRuntimePayloadSource<'_>],
) -> Result<Vec<u8>, String> {
    let source_map = validated_source_map(sources)?;
    let mut archive_cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut archive_cursor);
        let options = FileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o600);
        for (path, bytes) in &source_map {
            writer
                .start_file(path, options)
                .map_err(|error| format!("start runtime payload member {path}: {error}"))?;
            writer
                .write_all(bytes)
                .map_err(|error| format!("write runtime payload member {path}: {error}"))?;
        }
        writer
            .finish()
            .map_err(|error| format!("finish runtime payload ZIP: {error}"))?;
    }
    let archive = archive_cursor.into_inner();
    let archive_hash = Sha256::digest(&archive);
    let manifest = PayloadManifest {
        schema_version: PAYLOAD_FORMAT_VERSION,
        files: source_map
            .iter()
            .map(|(path, bytes)| PayloadManifestFile {
                path: path.clone(),
                sha256: sha256_hex(bytes),
            })
            .collect(),
    };
    let manifest_bytes = serde_json::to_vec(&manifest)
        .map_err(|error| format!("serialize runtime payload manifest: {error}"))?;

    let archive_len = u64::try_from(archive.len())
        .map_err(|_| "runtime payload archive is too large".to_string())?;
    let manifest_len = u64::try_from(manifest_bytes.len())
        .map_err(|_| "runtime payload manifest is too large".to_string())?;
    let mut output = Vec::with_capacity(
        base_executable.len() + archive.len() + manifest_bytes.len() + PAYLOAD_TRAILER_LEN,
    );
    output.extend_from_slice(base_executable);
    output.extend_from_slice(&archive);
    output.extend_from_slice(&manifest_bytes);
    output.extend_from_slice(PAYLOAD_MAGIC);
    output.extend_from_slice(&PAYLOAD_FORMAT_VERSION.to_le_bytes());
    output.extend_from_slice(&archive_len.to_le_bytes());
    output.extend_from_slice(&manifest_len.to_le_bytes());
    output.extend_from_slice(&archive_hash);
    Ok(output)
}

pub fn parse_embedded_runtime_payload(
    executable_bytes: &[u8],
) -> Result<EmbeddedRuntimePayload, String> {
    if executable_bytes.len() < PAYLOAD_TRAILER_LEN {
        return Err("Talk executable does not contain a complete runtime payload trailer".into());
    }
    let trailer_start = executable_bytes.len() - PAYLOAD_TRAILER_LEN;
    let trailer = &executable_bytes[trailer_start..];
    if &trailer[..8] != PAYLOAD_MAGIC {
        return Err("Talk executable runtime payload magic is missing".into());
    }
    let version = read_u32_le(&trailer[8..12]);
    if version != PAYLOAD_FORMAT_VERSION {
        return Err(format!(
            "unsupported Talk runtime payload version {version}"
        ));
    }
    let archive_len = usize::try_from(read_u64_le(&trailer[12..20]))
        .map_err(|_| "Talk runtime payload archive length is invalid".to_string())?;
    let manifest_len = usize::try_from(read_u64_le(&trailer[20..28]))
        .map_err(|_| "Talk runtime payload manifest length is invalid".to_string())?;
    let content_len = archive_len
        .checked_add(manifest_len)
        .ok_or_else(|| "Talk runtime payload lengths overflow".to_string())?;
    let archive_start = trailer_start
        .checked_sub(content_len)
        .ok_or_else(|| "Talk runtime payload lengths exceed executable size".to_string())?;
    let manifest_start = archive_start + archive_len;
    let archive = executable_bytes[archive_start..manifest_start].to_vec();
    let manifest_bytes = &executable_bytes[manifest_start..trailer_start];
    let expected_archive_hash = &trailer[28..60];
    let actual_archive_hash = Sha256::digest(&archive);
    if actual_archive_hash.as_slice() != expected_archive_hash {
        return Err("Talk runtime payload archive SHA-256 mismatch".into());
    }

    let manifest: PayloadManifest = serde_json::from_slice(manifest_bytes)
        .map_err(|error| format!("parse Talk runtime payload manifest: {error}"))?;
    if manifest.schema_version != PAYLOAD_FORMAT_VERSION {
        return Err(format!(
            "unsupported Talk runtime payload manifest version {}",
            manifest.schema_version
        ));
    }
    let manifest_files = validated_manifest_map(&manifest)?;
    validate_zip_members(&archive, &manifest_files)?;

    Ok(EmbeddedRuntimePayload {
        archive_sha256: sha256_hex(&archive),
        files: manifest_files
            .into_iter()
            .map(|(path, sha256)| EmbeddedRuntimePayloadFile {
                path: PathBuf::from(path),
                sha256,
            })
            .collect(),
        archive,
    })
}

pub fn extract_embedded_runtime_payload(
    executable_bytes: &[u8],
    runtime_root: &Path,
) -> Result<PathBuf, String> {
    let payload = parse_embedded_runtime_payload(executable_bytes)?;
    let destination = runtime_root.join(&payload.archive_sha256);
    if verified_runtime_matches(&destination, &payload)? {
        return Ok(destination);
    }

    fs::create_dir_all(runtime_root)
        .map_err(|error| format!("create Talk runtime root {}: {error}", runtime_root.display()))?;
    let temp_dir = runtime_root.join(format!(
        ".{}.tmp-{}-{}",
        payload.archive_sha256,
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("resolve Talk runtime extraction timestamp: {error}"))?
            .as_nanos()
    ));
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).map_err(|error| {
            format!(
                "remove stale Talk runtime temp directory {}: {error}",
                temp_dir.display()
            )
        })?;
    }

    let extraction_result = extract_payload_to_directory(&payload, &temp_dir);
    if let Err(error) = extraction_result {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(error);
    }

    if destination.exists() {
        if verified_runtime_matches(&destination, &payload)? {
            fs::remove_dir_all(&temp_dir).map_err(|error| {
                format!(
                    "remove duplicate Talk runtime temp directory {}: {error}",
                    temp_dir.display()
                )
            })?;
            return Ok(destination);
        }
        fs::remove_dir_all(&destination).map_err(|error| {
            format!(
                "remove invalid Talk runtime directory {}: {error}",
                destination.display()
            )
        })?;
    }
    fs::rename(&temp_dir, &destination).map_err(|error| {
        format!(
            "activate Talk runtime {} -> {}: {error}",
            temp_dir.display(),
            destination.display()
        )
    })?;
    Ok(destination)
}

fn validated_source_map(
    sources: &[EmbeddedRuntimePayloadSource<'_>],
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut source_map = BTreeMap::new();
    for source in sources {
        validate_member_path(source.path)?;
        if source_map
            .insert(source.path.to_string(), source.bytes.to_vec())
            .is_some()
        {
            return Err(format!(
                "duplicate Talk runtime payload member {}",
                source.path
            ));
        }
    }
    validate_expected_member_set(source_map.keys().map(String::as_str))?;
    Ok(source_map)
}

fn validated_manifest_map(
    manifest: &PayloadManifest,
) -> Result<BTreeMap<String, String>, String> {
    let mut files = BTreeMap::new();
    for file in &manifest.files {
        validate_member_path(&file.path)?;
        validate_sha256_hex(&file.sha256)?;
        if files
            .insert(file.path.clone(), file.sha256.to_ascii_lowercase())
            .is_some()
        {
            return Err(format!(
                "duplicate Talk runtime payload manifest member {}",
                file.path
            ));
        }
    }
    validate_expected_member_set(files.keys().map(String::as_str))?;
    Ok(files)
}

fn validate_zip_members(
    archive_bytes: &[u8],
    manifest_files: &BTreeMap<String, String>,
) -> Result<(), String> {
    let cursor = Cursor::new(archive_bytes);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|error| format!("open Talk runtime payload ZIP: {error}"))?;
    let mut seen = BTreeSet::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| format!("read Talk runtime payload ZIP member {index}: {error}"))?;
        let name = file.name().to_string();
        validate_member_path(&name)?;
        if !seen.insert(name.clone()) {
            return Err(format!("duplicate Talk runtime payload ZIP member {name}"));
        }
        let expected_hash = manifest_files
            .get(&name)
            .ok_or_else(|| format!("unexpected Talk runtime payload ZIP member {name}"))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| format!("read Talk runtime payload member {name}: {error}"))?;
        if sha256_hex(&bytes) != *expected_hash {
            return Err(format!(
                "Talk runtime payload member {name} SHA-256 mismatch"
            ));
        }
    }
    let expected = manifest_files.keys().cloned().collect::<BTreeSet<_>>();
    if seen != expected {
        return Err("Talk runtime payload ZIP member set does not match manifest".into());
    }
    Ok(())
}

fn extract_payload_to_directory(
    payload: &EmbeddedRuntimePayload,
    destination: &Path,
) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| {
        format!(
            "create Talk runtime extraction directory {}: {error}",
            destination.display()
        )
    })?;
    let mut archive = ZipArchive::new(Cursor::new(&payload.archive))
        .map_err(|error| format!("open Talk runtime payload ZIP: {error}"))?;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| format!("read Talk runtime payload ZIP member {index}: {error}"))?;
        let name = file.name().to_string();
        validate_member_path(&name)?;
        let output_path = destination.join(&name);
        let mut output = fs::File::create(&output_path).map_err(|error| {
            format!(
                "create Talk runtime payload member {}: {error}",
                output_path.display()
            )
        })?;
        std::io::copy(&mut file, &mut output).map_err(|error| {
            format!(
                "extract Talk runtime payload member {}: {error}",
                output_path.display()
            )
        })?;
    }
    for file in &payload.files {
        let output_path = destination.join(&file.path);
        let bytes = fs::read(&output_path).map_err(|error| {
            format!(
                "read extracted Talk runtime payload member {}: {error}",
                output_path.display()
            )
        })?;
        if sha256_hex(&bytes) != file.sha256 {
            return Err(format!(
                "extracted Talk runtime payload member {} SHA-256 mismatch",
                file.path.display()
            ));
        }
    }
    let marker = VerifiedRuntimeMarker {
        schema_version: PAYLOAD_FORMAT_VERSION,
        archive_sha256: payload.archive_sha256.clone(),
    };
    let marker_bytes = serde_json::to_vec_pretty(&marker)
        .map_err(|error| format!("serialize Talk runtime verification marker: {error}"))?;
    fs::write(destination.join(VERIFIED_MARKER_NAME), marker_bytes).map_err(|error| {
        format!(
            "write Talk runtime verification marker in {}: {error}",
            destination.display()
        )
    })?;
    Ok(())
}

fn verified_runtime_matches(
    directory: &Path,
    payload: &EmbeddedRuntimePayload,
) -> Result<bool, String> {
    if !directory.is_dir() {
        return Ok(false);
    }
    let marker_path = directory.join(VERIFIED_MARKER_NAME);
    let marker_bytes = match fs::read(&marker_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "read Talk runtime verification marker {}: {error}",
                marker_path.display()
            ))
        }
    };
    let marker: VerifiedRuntimeMarker = match serde_json::from_slice(&marker_bytes) {
        Ok(marker) => marker,
        Err(_) => return Ok(false),
    };
    if marker.schema_version != PAYLOAD_FORMAT_VERSION
        || marker.archive_sha256 != payload.archive_sha256
    {
        return Ok(false);
    }
    for file in &payload.files {
        let path = directory.join(&file.path);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                return Err(format!(
                    "read cached Talk runtime member {}: {error}",
                    path.display()
                ))
            }
        };
        if sha256_hex(&bytes) != file.sha256 {
            return Ok(false);
        }
    }
    Ok(true)
}

fn validate_member_path(path: &str) -> Result<(), String> {
    let path_value = Path::new(path);
    let mut components = path_value.components();
    let valid = matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && !path.is_empty();
    if !valid {
        return Err(format!(
            "Talk runtime payload member must be a single relative file name: {path}"
        ));
    }
    if !EXPECTED_RUNTIME_FILES.contains(&path) {
        return Err(format!("unexpected Talk runtime payload member {path}"));
    }
    Ok(())
}

fn validate_expected_member_set<'a>(paths: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let actual = paths.map(str::to_string).collect::<BTreeSet<_>>();
    let expected = EXPECTED_RUNTIME_FILES
        .iter()
        .map(|path| path.to_string())
        .collect::<BTreeSet<_>>();
    if actual != expected {
        let missing = expected.difference(&actual).cloned().collect::<Vec<_>>();
        let unexpected = actual.difference(&expected).cloned().collect::<Vec<_>>();
        return Err(format!(
            "Talk runtime payload member set mismatch; missing={missing:?}, unexpected={unexpected:?}"
        ));
    }
    Ok(())
}

fn validate_sha256_hex(value: &str) -> Result<(), String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("invalid Talk runtime payload SHA-256 {value}"));
    }
    Ok(())
}

fn read_u32_le(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("fixed u32 trailer slice"))
}

fn read_u64_le(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("fixed u64 trailer slice"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
