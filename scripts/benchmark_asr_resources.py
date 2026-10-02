#!/usr/bin/env python3
"""Measure a local Talk worker with identical public WAVs, without cloud calls.

Requires psutil and websockets. CPU/RSS refer to the worker, not this client.
Process cold start uses an OS-cached model; it does not flush the filesystem cache.
"""
import argparse
import asyncio
import base64
import datetime
import hashlib
import json
import platform
import re
import socket
import subprocess
import time
import unicodedata
import wave
from pathlib import Path



def normalized(text):
    return " ".join("".join(c for c in text.lower()
                            if not unicodedata.category(c).startswith("P")).split())


def distance(left, right):
    row = list(range(len(right) + 1))
    for i, a in enumerate(left, 1):
        current = [i]
        for j, b in enumerate(right, 1):
            current.append(min(current[-1] + 1, row[j] + 1, row[j-1] + (a != b)))
        row = current
    return row[-1]


def accuracy(item, text):
    ref, hyp = normalized(item["reference"]), normalized(text)
    chars = ref.replace(" ", "")
    result = {"character_errors": distance(chars, hyp.replace(" ", "")),
              "reference_characters": len(chars)}
    if item.get("language") == "en":
        result.update(word_errors=distance(ref.split(), hyp.split()),
                      reference_words=len(ref.split()))
    return result


def process_stats(process):
    cpu = process.cpu_times()
    mem = process.memory_info()
    result = {"cpu_seconds": cpu.user + cpu.system, "rss_bytes": mem.rss,
              "threads": process.num_threads()}
    if platform.system() == "Linux":
        status = Path(f"/proc/{process.pid}/status").read_text(encoding="utf-8")
        hwm = next((line.split()[1] for line in status.splitlines()
                    if line.startswith("VmHWM:")), None)
        result["process_peak_rss_bytes"] = int(hwm) * 1024 if hwm else None
    else:
        result["process_peak_rss_bytes"] = getattr(mem, "peak_wset", None)
    return result


async def measure(args, endpoint, item, process, repeat, warmup=False):
    import websockets
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*\.wav", item["file"]):
        raise ValueError("corpus filenames must be plain WAV filenames")
    path = args.corpus_manifest.parent / item["file"]
    if path.is_symlink() or path.stat().st_size > 32 * 1024 * 1024:
        raise ValueError("WAV must be a regular file no larger than 32 MiB")
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != item["sha256"]:
        raise ValueError(f"input hash mismatch: {item['id']}")
    with wave.open(str(path)) as wav:
        if (wav.getnchannels(), wav.getsampwidth(), wav.getframerate()) != (1, 2, 16000):
            raise ValueError("requires 16 kHz mono S16 PCM")
        if not 0 < wav.getnframes() <= 16000 * 900:
            raise ValueError("WAV duration must be between zero and 15 minutes")
        raw = wav.readframes(wav.getnframes())
    duration = len(raw) / 32000
    if not raw:
        raise ValueError("empty audio is not a latency/RTF sample")
    before = process_stats(process)
    started = time.monotonic()
    events = []
    async with websockets.connect(endpoint, open_timeout=5, close_timeout=1) as ws:
        await ws.send(json.dumps({"type": "start", "session_id": "resource-bench",
                                  "sample_rate_hz": 16000, "channels": 1}))
        ready = json.loads(await asyncio.wait_for(ws.recv(), 10))
        if ready["type"] != "ready":
            raise RuntimeError(f"unexpected response: {ready}")
        ready_at = time.monotonic()

        async def receive():
            while True:
                event = json.loads(await ws.recv())
                received = time.monotonic()
                events.append({"elapsed_ms": (received - started) * 1000, **event})
                if event["type"] == "error":
                    raise RuntimeError(f"worker error: {event}")
                if event["type"] == "final":
                    return received, event["text"]

        async def transfer():
            size = args.chunk_ms * 32
            for sequence, offset in enumerate(range(0, len(raw), size)):
                end = min(offset + size, len(raw))
                if args.pacing == "realtime":
                    await asyncio.sleep(max(0, ready_at + end / 32000 - time.monotonic()))
                await ws.send(json.dumps({"type": "audio", "session_id": "resource-bench",
                                         "sequence": sequence,
                                         "pcm_base64": base64.b64encode(raw[offset:end]).decode()}))
            stopped = time.monotonic()
            await ws.send(json.dumps({"type": "stop", "session_id": "resource-bench"}))
            return stopped

        reader = asyncio.create_task(receive())
        try:
            stopped = await asyncio.wait_for(transfer(), max(30, duration * 4))
            finished, text = await asyncio.wait_for(reader, max(30, duration * 4))
        finally:
            if not reader.done():
                reader.cancel()
            await asyncio.gather(reader, return_exceptions=True)
        after = process_stats(process)
    return {"sample_id": item["id"], "repeat": repeat, "warmup": warmup,
            "audio_sha256": digest, "audio_duration_ms": duration * 1000,
            "connect_ready_ms": (ready_at - started) * 1000,
            "first_partial_ms": next((e["elapsed_ms"] for e in events
                                      if e["type"] == "partial"), None),
            "stop_to_final_ms": (finished - stopped) * 1000,
            "final_latency_ms": (finished - started) * 1000,
            "rtf": (finished - started) / duration,
            "worker_cpu_seconds": after["cpu_seconds"] - before["cpu_seconds"],
            "worker_rss_bytes": after["rss_bytes"],
            "worker_process_peak_rss_bytes": after["process_peak_rss_bytes"],
            "text": text, "accuracy": accuracy(item, text), "events": events}


def write(args, report):
    if args.output_json.is_symlink():
        raise ValueError("refusing symbolic link output")
    args.output_json.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def reserve_output(path):
    """Own a new report path without truncating an existing file or symlink."""
    with path.open("x", encoding="utf-8") as output:
        output.write("{}\n")


async def run(args, endpoint, corpus, process, report):
    report["warmup"] = await measure(args, endpoint, corpus[0], process, -1, True)
    before = process_stats(process)
    await asyncio.sleep(2)
    report["idle_2s_cpu_seconds"] = process_stats(process)["cpu_seconds"] - before["cpu_seconds"]
    for repeat in range(args.repetitions):
        for item in corpus:
            result = await measure(args, endpoint, item, process, repeat)
            report["results"].append(result)
            write(args, report)
            print(json.dumps({k: v for k, v in result.items() if k != "events"},
                             ensure_ascii=True), flush=True)


def main():
    import psutil
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("worker", "model-dir", "corpus-manifest", "output-json"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--code-revision", required=True)
    parser.add_argument("--num-threads", type=int, default=2)
    parser.add_argument("--chunk-ms", type=int, default=48)
    parser.add_argument("--repetitions", type=int, default=2)
    parser.add_argument("--pacing", choices=["bulk", "realtime"], default="bulk")
    parser.add_argument("--decoding-method", default="greedy_search")
    parser.add_argument("--provider", default="cpu")
    parser.add_argument("--sample-id", action="append", default=[])
    args = parser.parse_args()
    if not (1 <= args.num_threads <= 16 and 1 <= args.chunk_ms <= 1000
            and 1 <= args.repetitions <= 20):
        parser.error("invalid thread, chunk, or repetition bounds")
    corpus = json.loads(args.corpus_manifest.read_text(encoding="utf-8"))
    if not isinstance(corpus, list) or not 1 <= len(corpus) <= 1000:
        parser.error("corpus must contain 1..1000 items")
    if args.sample_id:
        corpus = [item for item in corpus if item["id"] in args.sample_id]
    if not corpus:
        parser.error("empty corpus")
    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    log_path = args.output_json.with_suffix(".worker.log")
    if log_path.exists() or log_path.is_symlink():
        parser.error("worker log already exists; choose a new output filename")
    reserve_output(args.output_json)
    report = {"label": args.label, "code_revision": args.code_revision,
              "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "probe_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "platform": platform.platform(), "worker_sha256": hashlib.sha256(args.worker.read_bytes()).hexdigest(),
              "model_files_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                                     for p in args.model_dir.iterdir()
                                     if p.suffix == ".onnx" or p.name == "tokens.txt"},
              "config": {"num_threads": args.num_threads, "chunk_ms": args.chunk_ms,
                         "pacing": args.pacing, "repetitions": args.repetitions,
                         "decoding_method": args.decoding_method, "tail_padding_ms": 1000,
                         "provider": args.provider},
              "corpus": corpus, "results": []}
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    command = [str(args.worker.resolve()), "--mode", "sherpa-online", "--model",
               "zipformer-zh-en-punct-int8-480ms", "--tokens", str(args.model_dir / "tokens.txt"),
               "--encoder", str(args.model_dir / "encoder.int8.onnx"), "--decoder",
               str(args.model_dir / "decoder.onnx"), "--joiner", str(args.model_dir / "joiner.int8.onnx"),
               "--num-threads", str(args.num_threads), "--bind", f"127.0.0.1:{port}",
               "--decoding-method", args.decoding_method, "--tail-padding-ms", "1000",
               "--provider", args.provider]
    started = time.monotonic()
    with log_path.open("x", encoding="utf-8") as log:
        worker = subprocess.Popen(command, stdout=log, stderr=log)
        process = psutil.Process(worker.pid)
        try:
            while True:
                if worker.poll() is not None:
                    raise RuntimeError("worker exited during initialization; inspect log")
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=0.1):
                        break
                except OSError:
                    if time.monotonic() - started > 30:
                        raise TimeoutError("worker startup exceeded 30 seconds")
                    time.sleep(0.01)
            report["process_startup_ms"] = (time.monotonic() - started) * 1000
            report["after_startup"] = process_stats(process)
            asyncio.run(run(args, f"ws://127.0.0.1:{port}/asr", corpus, process, report))
            report["after_all_samples"] = process_stats(process)
        finally:
            worker.terminate()
            try:
                worker.wait(timeout=5)
            except subprocess.TimeoutExpired:
                worker.kill()
                worker.wait()
            write(args, report)


if __name__ == "__main__":
    main()
