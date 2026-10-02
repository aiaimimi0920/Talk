import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).parents[1] / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


bench = load("benchmark_asr_resources")
prepare = load("prepare_public_asr_corpus")


class MetricsContract(unittest.TestCase):
    def test_cer_and_wer_keep_distinct_denominators(self):
        score = bench.accuracy({"reference": "The mantel board.", "language": "en"},
                               "the mantelboard")
        self.assertEqual(score, {"character_errors": 0, "reference_characters": 14,
                                 "word_errors": 2, "reference_words": 3})

    def test_mandarin_is_not_scored_as_a_single_word(self):
        score = bench.accuracy({"reference": "你好，世界！", "language": "zh"}, "你好世界")
        self.assertEqual(score, {"character_errors": 0, "reference_characters": 4})

    def test_edit_distance_counts_insertions_deletions_and_substitutions(self):
        self.assertEqual(bench.distance("", "abc"), 3)
        self.assertEqual(bench.distance("abc", ""), 3)
        self.assertEqual(bench.distance("kitten", "sitting"), 3)
        self.assertEqual(bench.normalized("HELLO…  world!"), "hello world")


class CorpusIntegrity(unittest.TestCase):
    def test_existing_probe_report_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "report.json"
            target.write_text("prior report")
            with self.assertRaises(FileExistsError):
                bench.reserve_output(target)
            self.assertEqual(target.read_text(encoding="utf-8"), "prior report")

    def test_dangling_symlink_is_rejected_without_creating_target(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "outside.wav"
            link = Path(directory) / "sample.wav"
            try:
                link.symlink_to(target)
            except (OSError, NotImplementedError):
                self.skipTest("creating symlinks requires platform permission")
            with self.assertRaisesRegex(ValueError, "symbolic link"):
                prepare.verified_write(link, b"valid", hashlib.sha256(b"valid").hexdigest())
            self.assertFalse(target.exists())

    def test_checksum_failure_never_writes_a_file(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "sample.wav"
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                prepare.verified_write(target, b"wrong", hashlib.sha256(b"expected").hexdigest())
            self.assertFalse(target.exists())

    def test_existing_different_file_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "sample.wav"
            target.write_bytes(b"existing")
            with self.assertRaisesRegex(ValueError, "existing file differs"):
                prepare.verified_write(target, b"correct", hashlib.sha256(b"correct").hexdigest())
            self.assertEqual(target.read_bytes(), b"existing")

    def test_manifest_rejects_path_escape_before_downloading(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest = Path(directory) / "manifest.json"
            manifest.write_text(json.dumps([{"file": "../escape.wav"}]))
            with patch.object(prepare, "download") as download:
                with self.assertRaisesRegex(ValueError, "plain filenames"):
                    prepare.prepare(manifest, Path(directory) / "corpus")
                download.assert_not_called()

    def test_windows_path_escape_is_rejected_on_all_platforms(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest = Path(directory) / "manifest.json"
            manifest.write_text(json.dumps([{"file": "..\\escape.wav"}]))
            with self.assertRaisesRegex(ValueError, "plain filenames"):
                prepare.prepare(manifest, Path(directory) / "corpus")

    def test_download_reads_at_most_limit_plus_one_and_rejects_oversize(self):
        response = unittest.mock.MagicMock()
        response.__enter__.return_value = response
        response.read.return_value = b"12345"
        with patch.object(prepare.urllib.request, "urlopen", return_value=response):
            with self.assertRaisesRegex(ValueError, "download byte bound"):
                prepare.download("https://example.invalid/public.wav", byte_limit=4)
        response.read.assert_called_once_with(5)

    def test_pinned_corpus_has_85_unique_items_and_explicit_languages(self):
        corpus = json.loads((Path(__file__).parents[2] /
                             "docs/asr-benchmarks/public-corpus-85.json").read_text(encoding="utf-8"))
        self.assertEqual(len(corpus), 85)
        self.assertEqual(len({item["id"] for item in corpus}), 85)
        self.assertEqual(sum(item["language"] == "zh" for item in corpus), 11)
        self.assertEqual(sum(item["language"] == "en" for item in corpus), 74)
        for item in corpus:
            self.assertTrue(item["reference"])
            self.assertEqual(len(item["sha256"]), 64)
            self.assertTrue(item["download"]["url"].startswith("https://"))


if __name__ == "__main__":
    unittest.main()
