use bzip2::read::BzDecoder;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use tar::Archive;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

const MODEL_MANIFEST_FILE: &str = "model-manifest.json";
const MODEL_DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const MODEL_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSpec {
    pub id: String,
    pub url: String,
    pub archive_name: String,
    pub sha256: String,
    pub required_files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstalledModelManifest {
    schema_version: u32,
    model_id: String,
    archive_sha256: String,
}

pub fn default_product_local_asr_model_spec() -> ModelSpec {
    ModelSpec {
        id: "zipformer-zh-en-punct-int8-480ms".to_string(),
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8-2026-06-05.tar.bz2".to_string(),
        archive_name: "sherpa-onnx-x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8-2026-06-05.tar.bz2".to_string(),
        sha256: "fa5f63d618e5a01526e275a358bb7772e403f84808a4769fba52cffd8160bf74".to_string(),
        required_files: vec![
            "tokens.txt".to_string(),
            "encoder.int8.onnx".to_string(),
            "decoder.onnx".to_string(),
            "joiner.int8.onnx".to_string(),
        ],
    }
}

pub fn resolve_talk_data_root() -> Result<PathBuf, String> {
    let base = env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .map(|home| home.join(".local").join("share"))
        })
        .ok_or_else(|| "Talk cannot resolve LOCALAPPDATA or HOME for product data".to_string())?;
    Ok(base.join("Talk"))
}

pub fn validate_installed_model(spec: &ModelSpec, model_dir: &Path) -> Result<(), String> {
    validate_model_spec(spec)?;
    if !model_dir.is_dir() {
        return Err(format!(
            "Talk model directory does not exist: {}",
            model_dir.display()
        ));
    }
    let present_files = find_required_files(model_dir, &spec.required_files)?;
    if let Some(required) = spec
        .required_files
        .iter()
        .find(|required| !present_files.contains(required.as_str()))
    {
        return Err(format!(
            "Talk model {} is missing required file {required}",
            spec.id
        ));
    }
    let marker_path = model_dir.join(MODEL_MANIFEST_FILE);
    let marker_bytes = fs::read(&marker_path).map_err(|error| {
        format!(
            "read Talk model manifest {}: {error}",
            marker_path.display()
        )
    })?;
    let marker: InstalledModelManifest =
        serde_json::from_slice(&marker_bytes).map_err(|error| {
            format!(
                "parse Talk model manifest {}: {error}",
                marker_path.display()
            )
        })?;
    if marker.schema_version != 1
        || marker.model_id != spec.id
        || marker.archive_sha256 != spec.sha256.to_ascii_lowercase()
    {
        return Err(format!(
            "Talk model manifest does not match catalog entry {}",
            spec.id
        ));
    }
    Ok(())
}

pub fn install_model_from_reader<R: Read>(
    spec: &ModelSpec,
    mut reader: R,
    model_root: &Path,
) -> Result<PathBuf, String> {
    validate_model_spec(spec)?;
    let destination = model_root.join(&spec.id);
    if validate_installed_model(spec, &destination).is_ok() {
        return Ok(destination);
    }
    let mut archive_bytes = Vec::new();
    reader
        .read_to_end(&mut archive_bytes)
        .map_err(|error| format!("read Talk model archive {}: {error}", spec.archive_name))?;
    let actual_hash = sha256_hex(&archive_bytes);
    if actual_hash != spec.sha256.to_ascii_lowercase() {
        return Err(format!(
            "Talk model archive SHA-256 mismatch for {}: expected {}, got {actual_hash}",
            spec.id, spec.sha256
        ));
    }
    install_verified_model_from_reader(spec, Cursor::new(archive_bytes), model_root)
}

pub async fn download_and_install_model(
    spec: &ModelSpec,
    model_root: &Path,
) -> Result<PathBuf, String> {
    validate_model_spec(spec)?;
    let destination = model_root.join(&spec.id);
    if validate_installed_model(spec, &destination).is_ok() {
        return Ok(destination);
    }
    fs::create_dir_all(model_root)
        .map_err(|error| format!("create Talk model root {}: {error}", model_root.display()))?;
    let downloads = model_root.join("_downloads");
    fs::create_dir_all(&downloads).map_err(|error| {
        format!(
            "create Talk model download directory {}: {error}",
            downloads.display()
        )
    })?;
    let partial_path = downloads.join(format!(
        "{}.{}.partial",
        spec.archive_name,
        Uuid::new_v4().simple()
    ));
    let install_result = async {
        let client = reqwest::Client::builder()
            .connect_timeout(MODEL_DOWNLOAD_CONNECT_TIMEOUT)
            .timeout(MODEL_DOWNLOAD_TIMEOUT)
            .build()
            .map_err(|error| format!("configure Talk model download client: {error}"))?;
        let response = client
            .get(&spec.url)
            .send()
            .await
            .map_err(|error| format!("download Talk model {}: {error}", spec.id))?
            .error_for_status()
            .map_err(|error| format!("download Talk model {}: {error}", spec.id))?;
        let mut output = tokio::fs::File::create(&partial_path)
            .await
            .map_err(|error| {
                format!(
                    "write Talk model partial archive {}: {error}",
                    partial_path.display()
                )
            })?;
        let mut stream = response.bytes_stream();
        let mut hasher = Sha256::new();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result
                .map_err(|error| format!("read Talk model download {}: {error}", spec.id))?;
            hasher.update(&chunk);
            output.write_all(&chunk).await.map_err(|error| {
                format!(
                    "write Talk model partial archive {}: {error}",
                    partial_path.display()
                )
            })?;
        }
        output.flush().await.map_err(|error| {
            format!(
                "flush Talk model partial archive {}: {error}",
                partial_path.display()
            )
        })?;
        drop(output);
        let actual_hash = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if actual_hash != spec.sha256.to_ascii_lowercase() {
            return Err(format!(
                "Talk model archive SHA-256 mismatch for {}: expected {}, got {actual_hash}",
                spec.id, spec.sha256
            ));
        }

        let install_spec = spec.clone();
        let install_root = model_root.to_path_buf();
        let install_archive = partial_path.clone();
        tokio::task::spawn_blocking(move || {
            let file = fs::File::open(&install_archive).map_err(|error| {
                format!(
                    "open Talk model partial archive {}: {error}",
                    install_archive.display()
                )
            })?;
            install_verified_model_from_reader(&install_spec, file, &install_root)
        })
        .await
        .map_err(|error| format!("join Talk model installation worker: {error}"))?
    }
    .await;

    let cleanup_error = match tokio::fs::remove_file(&partial_path).await {
        Ok(()) => None,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => Some(format!(
            "remove Talk model partial archive {}: {error}",
            partial_path.display()
        )),
    };
    finish_model_install(install_result, cleanup_error)
}

fn finish_model_install(
    install_result: Result<PathBuf, String>,
    cleanup_error: Option<String>,
) -> Result<PathBuf, String> {
    match (install_result, cleanup_error) {
        (Ok(destination), Some(cleanup_error)) => {
            eprintln!(
                "Talk model install succeeded, but partial archive cleanup failed: {cleanup_error}"
            );
            Ok(destination)
        }
        (Ok(destination), None) => Ok(destination),
        (Err(error), None) => Err(error),
        (Err(error), Some(cleanup_error)) => Err(format!("{error}; {cleanup_error}")),
    }
}

fn install_verified_model_from_reader<R: Read>(
    spec: &ModelSpec,
    reader: R,
    model_root: &Path,
) -> Result<PathBuf, String> {
    fs::create_dir_all(model_root)
        .map_err(|error| format!("create Talk model root {}: {error}", model_root.display()))?;
    let temp_dir = model_root.join(format!(".{}.tmp-{}", spec.id, Uuid::new_v4().simple()));
    fs::create_dir_all(&temp_dir).map_err(|error| {
        format!(
            "create Talk model temp directory {}: {error}",
            temp_dir.display()
        )
    })?;

    let extraction_result = extract_model_archive(reader, &temp_dir);
    let package_root = match extraction_result {
        Ok(root) => root,
        Err(error) => {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(error);
        }
    };
    let present_files = match find_required_files(&package_root, &spec.required_files) {
        Ok(present_files) => present_files,
        Err(error) => {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(error);
        }
    };
    if let Some(required) = spec
        .required_files
        .iter()
        .find(|required| !present_files.contains(required.as_str()))
    {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(format!(
            "Talk model archive {} is missing required file {required}",
            spec.archive_name
        ));
    }
    let marker = InstalledModelManifest {
        schema_version: 1,
        model_id: spec.id.clone(),
        archive_sha256: spec.sha256.to_ascii_lowercase(),
    };
    let marker_bytes = match serde_json::to_vec_pretty(&marker) {
        Ok(marker_bytes) => marker_bytes,
        Err(error) => {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(format!("serialize Talk model manifest: {error}"));
        }
    };
    if let Err(error) = fs::write(package_root.join(MODEL_MANIFEST_FILE), marker_bytes) {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(format!("write Talk model manifest: {error}"));
    }

    let destination = model_root.join(&spec.id);
    if validate_installed_model(spec, &destination).is_ok() {
        let _ = fs::remove_dir_all(&temp_dir);
        return Ok(destination);
    }

    let displaced_destination =
        model_root.join(format!(".{}.invalid-{}", spec.id, Uuid::new_v4().simple()));
    let displaced_destination = if destination.exists() {
        match fs::rename(&destination, &displaced_destination) {
            Ok(()) => Some(displaced_destination),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                if validate_installed_model(spec, &destination).is_ok() {
                    let _ = fs::remove_dir_all(&temp_dir);
                    return Ok(destination);
                }
                let _ = fs::remove_dir_all(&temp_dir);
                return Err(format!(
                    "quarantine invalid Talk model directory {}: {error}",
                    destination.display()
                ));
            }
        }
    } else {
        None
    };

    if let Err(error) = fs::rename(&package_root, &destination) {
        if validate_installed_model(spec, &destination).is_ok() {
            let _ = fs::remove_dir_all(&temp_dir);
            if let Some(displaced_destination) = displaced_destination {
                let _ = fs::remove_dir_all(displaced_destination);
            }
            return Ok(destination);
        }
        if !destination.exists() {
            if let Some(displaced_destination) = displaced_destination.as_ref() {
                let _ = fs::rename(displaced_destination, &destination);
            }
        }
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(format!(
            "activate Talk model {} -> {}: {error}",
            package_root.display(),
            destination.display()
        ));
    }

    if let Err(error) = validate_installed_model(spec, &destination) {
        let _ = fs::remove_dir_all(&destination);
        if let Some(displaced_destination) = displaced_destination.as_ref() {
            let _ = fs::rename(displaced_destination, &destination);
        }
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(error);
    }
    if let Some(displaced_destination) = displaced_destination {
        let _ = fs::remove_dir_all(displaced_destination);
    }
    let _ = fs::remove_dir_all(&temp_dir);
    Ok(destination)
}

fn extract_model_archive<R: Read>(reader: R, destination: &Path) -> Result<PathBuf, String> {
    let decoder = BzDecoder::new(reader);
    let mut archive = Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|error| format!("open Talk model tar.bz2 archive: {error}"))?;
    let mut top_level_names = BTreeSet::new();
    for entry_result in entries {
        let mut entry =
            entry_result.map_err(|error| format!("read Talk model archive entry: {error}"))?;
        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err("Talk model archive contains an unsupported link or special entry".into());
        }
        let path = entry
            .path()
            .map_err(|error| format!("read Talk model archive entry path: {error}"))?
            .into_owned();
        validate_archive_relative_path(&path)?;
        let top_level = path
            .components()
            .next()
            .and_then(|component| match component {
                Component::Normal(value) => Some(value.to_owned()),
                _ => None,
            })
            .ok_or_else(|| "Talk model archive entry has no top-level directory".to_string())?;
        top_level_names.insert(top_level);
        let output_path = destination.join(&path);
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "create Talk model archive directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        entry
            .unpack(&output_path)
            .map_err(|error| format!("extract Talk model entry {}: {error}", path.display()))?;
    }
    if top_level_names.len() != 1 {
        return Err(format!(
            "Talk model archive must contain one top-level directory, got {}",
            top_level_names.len()
        ));
    }
    let top_level = top_level_names
        .into_iter()
        .next()
        .expect("single top-level model directory");
    let package_root = destination.join(top_level);
    if !package_root.is_dir() {
        return Err(format!(
            "Talk model archive top-level path is not a directory: {}",
            package_root.display()
        ));
    }
    Ok(package_root)
}

fn validate_model_spec(spec: &ModelSpec) -> Result<(), String> {
    if spec.id.trim().is_empty()
        || spec.archive_name.trim().is_empty()
        || spec.required_files.is_empty()
    {
        return Err("Talk model catalog entry contains a blank required field".into());
    }
    if !spec.url.starts_with("https://") {
        return Err(format!(
            "Talk model catalog URL must use HTTPS: {}",
            spec.url
        ));
    }
    if spec.sha256.len() != 64 || !spec.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "Talk model catalog SHA-256 is invalid for {}",
            spec.id
        ));
    }
    for (field_name, value) in [
        ("id", spec.id.as_str()),
        ("archive_name", spec.archive_name.as_str()),
    ] {
        let path = Path::new(value);
        let mut components = path.components();
        if value.trim() != value
            || !matches!(components.next(), Some(Component::Normal(_)))
            || components.next().is_some()
        {
            return Err(format!(
                "Talk model catalog {field_name} must be a single safe path component: {value}"
            ));
        }
    }
    for required in &spec.required_files {
        let path = Path::new(required);
        let mut components = path.components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            return Err(format!(
                "Talk model required file must be a file name: {required}"
            ));
        }
    }
    Ok(())
}

fn validate_archive_relative_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(format!(
            "Talk model archive entry must be a non-empty relative path: {}",
            path.display()
        ));
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(format!(
                "Talk model archive entry must use a relative path without traversal: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn find_required_files(root: &Path, required_files: &[String]) -> Result<BTreeSet<String>, String> {
    let targets = required_files.iter().cloned().collect::<BTreeSet<_>>();
    let mut found = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory).map_err(|error| {
            format!("read Talk model directory {}: {error}", directory.display())
        })?;
        for entry_result in entries {
            let entry = entry_result.map_err(|error| {
                format!(
                    "read Talk model directory entry in {}: {error}",
                    directory.display()
                )
            })?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|error| {
                format!("read Talk model file type {}: {error}", path.display())
            })?;
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if targets.contains(&name) {
                    found.insert(name);
                    if found.len() == targets.len() {
                        return Ok(found);
                    }
                }
            }
        }
    }
    Ok(found)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::finish_model_install;
    use std::path::PathBuf;

    #[test]
    fn cleanup_failure_does_not_mask_a_successful_model_install() {
        let destination = PathBuf::from("model-destination");

        let result = finish_model_install(
            Ok(destination.clone()),
            Some("partial archive is locked".to_string()),
        );

        assert_eq!(result, Ok(destination));
    }

    #[test]
    fn cleanup_failure_is_appended_to_a_failed_model_install() {
        let result = finish_model_install(
            Err("archive extraction failed".to_string()),
            Some("partial archive is locked".to_string()),
        );

        assert_eq!(
            result,
            Err("archive extraction failed; partial archive is locked".to_string())
        );
    }
}
