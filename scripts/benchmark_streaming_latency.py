#!/usr/bin/env python3
"""Optional real-model probe: pace public WAV input like Talk's 48 ms live pump.

Requires the optional `websockets` Python package. It does not download data,
record a microphone, or contact an external ASR service. See the paired report
in docs/asr-benchmarks/streaming-tail-context-20261001.md for inputs and limits.
"""

import argparse
import asyncio
import base64
import hashlib
import json
import socket
import subprocess
import time
import wave
from pathlib import Path

import websockets


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worker", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--corpus-manifest", type=Path, required=True)
    parser.add_argument("--output-json", type=Path, required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--sample-id", action="append", default=[])
    parser.add_argument("--repetitions", type=int, default=2)
    parser.add_argument("--chunk-ms", type=int, default=48)
    parser.add_argument("--disable-nagle", action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument("--tail-padding-ms", type=int)
    args = parser.parse_args()
    if args.repetitions <= 0 or args.chunk_ms <= 0:
        parser.error("repetitions and chunk-ms must be positive")
    if args.tail_padding_ms is not None and not 0 <= args.tail_padding_ms <= 2000:
        parser.error("tail-padding-ms must be between 0 and 2000")
    return args


async def measure(args, endpoint, item, repeat):
    audio_path = args.corpus_manifest.parent / item["file"]
    with wave.open(str(audio_path)) as wav:
        if (wav.getnchannels(), wav.getsampwidth(), wav.getframerate()) != (1, 2, 16000):
            raise ValueError("the paired probe requires 16 kHz mono S16LE WAV")
        rate = wav.getframerate()
        raw = wav.readframes(wav.getnframes())
    async with websockets.connect(endpoint) as ws:
        ws.transport.get_extra_info("socket").setsockopt(
            socket.IPPROTO_TCP, socket.TCP_NODELAY, args.disable_nagle
        )
        await ws.send(json.dumps({"type": "start", "session_id": "live-bench",
                                  "sample_rate_hz": rate, "channels": 1}))
        ready = json.loads(await asyncio.wait_for(ws.recv(), timeout=5))
        if ready["type"] != "ready":
            raise RuntimeError(f"expected ready, received {ready['type']}")
        started = time.monotonic()
        events = []

        async def receive():
            while True:
                event = json.loads(await ws.recv())
                received = time.monotonic()
                events.append({"elapsed_ms": (received - started) * 1000, **event})
                if event["type"] == "error":
                    raise RuntimeError("worker returned an error")
                if event["type"] == "final":
                    return received, event["text"]

        reader = asyncio.create_task(receive())
        try:
            chunk_bytes = max(1, rate * args.chunk_ms // 1000) * 2
            for sequence, offset in enumerate(range(0, len(raw), chunk_bytes)):
                end = min(offset + chunk_bytes, len(raw))
                deadline = started + end / 2 / rate
                await asyncio.sleep(max(0, deadline - time.monotonic()))
                await ws.send(json.dumps({
                    "type": "audio", "session_id": "live-bench", "sequence": sequence,
                    "pcm_base64": base64.b64encode(raw[offset:end]).decode("ascii"),
                }))
            stopped = time.monotonic()
            await ws.send(json.dumps({"type": "stop", "session_id": "live-bench"}))
            finished, text = await asyncio.wait_for(reader, timeout=7)
        finally:
            if not reader.done():
                reader.cancel()
            await asyncio.gather(reader, return_exceptions=True)
        return {
            "variant": args.label, "sample_id": item["id"], "repeat": repeat,
            "audio_sha256": hashlib.sha256(audio_path.read_bytes()).hexdigest(),
            "first_partial_ms": next((e["elapsed_ms"] for e in events
                                      if e["type"] == "partial"), None),
            "stop_to_final_ms": (finished - stopped) * 1000,
            "final_latency_ms": (finished - started) * 1000,
            "text": text, "events": events,
        }


async def run(args, endpoint, corpus):
    results = []
    for repeat in range(args.repetitions):
        for item in corpus:
            result = await measure(args, endpoint, item, repeat)
            results.append(result)
            # Preserve completed measurements if a later sample fails.
            args.output_json.write_text(json.dumps(results, ensure_ascii=False, indent=2) + "\n")
            print({k: v for k, v in result.items() if k != "events"}, flush=True)


def main():
    args = arguments()
    corpus = json.loads(args.corpus_manifest.read_text())
    if args.sample_id:
        corpus = [item for item in corpus if item["id"] in args.sample_id]
    if not corpus:
        raise ValueError("no matching corpus samples")
    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    command = [str(args.worker.resolve()), "--mode", "sherpa-online", "--model",
               "zipformer-zh-en-punct-int8-480ms", "--tokens", str(args.model_dir / "tokens.txt"),
               "--encoder", str(args.model_dir / "encoder.int8.onnx"), "--decoder",
               str(args.model_dir / "decoder.onnx"), "--joiner", str(args.model_dir / "joiner.int8.onnx"),
               "--num-threads", "2", "--bind", f"127.0.0.1:{port}"]
    if args.tail_padding_ms is not None:
        command += ["--tail-padding-ms", str(args.tail_padding_ms)]
    with args.output_json.with_suffix(".worker.log").open("w") as log:
        worker = subprocess.Popen(command, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 30
            while True:
                if worker.poll() is not None:
                    raise RuntimeError("worker exited during initialization; inspect worker log")
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=0.1):
                        break
                except OSError:
                    if time.monotonic() >= deadline:
                        raise TimeoutError("worker did not start within 30 seconds")
                    time.sleep(0.1)
            asyncio.run(run(args, f"ws://127.0.0.1:{port}/asr", corpus))
        finally:
            worker.terminate()
            try:
                worker.wait(timeout=10)
            except subprocess.TimeoutExpired:
                worker.kill()
                worker.wait()


if __name__ == "__main__":
    main()
