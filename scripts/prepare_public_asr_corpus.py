#!/usr/bin/env python3
"""Prepare the checksum-pinned public regression corpus, without recording audio.

The LibriSpeech parquet inputs require pyarrow and soundfile. All downloads use
HTTPS, are checksum verified, and are limited to 16 MiB. Existing mismatched
files are rejected rather than overwritten. See docs/ASR_BENCHMARKING.md.
"""
import argparse
import hashlib
import io
import json
import re
import urllib.request
import wave
from pathlib import Path


def verified_write(path, data, expected):
    if hashlib.sha256(data).hexdigest() != expected:
        raise ValueError(f"checksum mismatch: {path.name}")
    if path.is_symlink():
        raise ValueError(f"refusing symbolic link: {path}")
    if path.exists():
        if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError(f"existing file differs: {path}")
        return
    with path.open("xb") as output:
        output.write(data)


def download(url, byte_limit=16 * 1024 * 1024):
    if not url.startswith("https://"):
        raise ValueError("only HTTPS corpus sources are supported")
    with urllib.request.urlopen(url, timeout=60) as response:
        data = response.read(byte_limit + 1)
    if len(data) > byte_limit:
        raise ValueError("corpus source exceeds the download byte bound")
    return data


def pcm_wav(audio):
    import soundfile
    samples, rate = soundfile.read(io.BytesIO(audio), dtype="int16")
    if rate != 16000 or samples.ndim != 1:
        raise ValueError("expected mono 16 kHz corpus source")
    output = io.BytesIO()
    with wave.open(output, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(rate)
        wav.writeframes(samples.astype("<i2").tobytes())
    return output.getvalue()


def prepare(manifest, output_dir):
    corpus = json.loads(manifest.read_text(encoding="utf-8"))
    if not isinstance(corpus, list) or not 1 <= len(corpus) <= 1000:
        raise ValueError("corpus must contain 1..1000 items")
    output_dir.mkdir(parents=True, exist_ok=True)
    parquet_rows = {}
    downloaded_bytes = 0

    def bounded_download(url):
        nonlocal downloaded_bytes
        remaining = 64 * 1024 * 1024 - downloaded_bytes
        if remaining <= 0:
            raise ValueError("corpus exceeds the 64 MiB total download bound")
        data = download(url, min(16 * 1024 * 1024, remaining))
        downloaded_bytes += len(data)
        return data

    for item in corpus:
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*\.wav", item["file"]):
            raise ValueError("corpus filenames must be plain filenames")
        if not re.fullmatch(r"[0-9a-f]{64}", item["sha256"]):
            raise ValueError("invalid WAV checksum")
        destination = output_dir / item["file"]
        if destination.is_symlink():
            raise ValueError(f"refusing symbolic link: {destination}")
        if destination.exists():
            verified_write(destination, destination.read_bytes(), item["sha256"])
            continue
        source = item["download"]
        if not re.fullmatch(r"[0-9a-f]{64}", source["sha256"]):
            raise ValueError("invalid source checksum")
        if "parquet_row" in source:
            key = source["sha256"]
            if key not in parquet_rows:
                import pyarrow.parquet
                archive = output_dir / (key + ".parquet")
                if archive.is_symlink():
                    raise ValueError(f"refusing symbolic link: {archive}")
                if archive.exists() and archive.stat().st_size > 16 * 1024 * 1024:
                    raise ValueError("cached source exceeds 16 MiB")
                data = archive.read_bytes() if archive.exists() else bounded_download(source["url"])
                verified_write(archive, data, key)
                parquet_rows[key] = pyarrow.parquet.read_table(archive).to_pylist()
            row = parquet_rows[key][source["parquet_row"]]
            if row["text"] != item["reference"]:
                raise ValueError(f"reference mismatch: {item['id']}")
            data = pcm_wav(row["audio"]["bytes"])
        else:
            data = bounded_download(source["url"])
        verified_write(destination, data, item["sha256"])
    copied_manifest = json.dumps(corpus, ensure_ascii=False, indent=2).encode() + b"\n"
    verified_write(output_dir / "corpus.json", copied_manifest,
                   hashlib.sha256(copied_manifest).hexdigest())
    return len(corpus)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    print(f"Prepared {prepare(args.manifest, args.output_dir)} verified WAV files")


if __name__ == "__main__":
    main()
