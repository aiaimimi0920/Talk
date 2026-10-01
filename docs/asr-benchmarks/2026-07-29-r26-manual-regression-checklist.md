# Talk r26 Manual Regression Checklist

Date baseline: 2026-07-29

Build under test:

- `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-accuracy-20260729-r26\Talk.exe`
- `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-accuracy-20260729-r26\talk.toml`

Primary evidence for this build:

- `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\_ci\talk-accuracy-20260729-r26\product-evidence.json`
- `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\_ci\talk-accuracy-20260729-r26\local-replay-evidence.json`

## 1. Test objective

This checklist is for real human validation after the r26 accuracy retuning work.

The current expectation is:

- the trusted faithful replay corpus stays fully green;
- the packaged release still behaves as local-first Talk;
- the recently fixed multilingual alias cases do not regress in live use.

## 2. Preflight

Before starting, verify:

1. Use the packaged r26 build above, not an older Talk release.
2. Prefer a quiet environment for the first pass.
3. Confirm the Windows default microphone is the microphone you want to test.
4. Prefer to have DashScope credentials available through either:
   - `TALK_PROVIDER_API_KEY`, or
   - the standard per-user DashScope credential file.
5. If credentials are unavailable, still run the checklist, but mark the run as `local-only`.

Expected runtime artifact roots after launch:

- logs:
  - `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-accuracy-20260729-r26\.runtime\talk-desktop\logs`
- temporary audio:
  - `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-accuracy-20260729-r26\.runtime\talk-desktop\audio`

## 3. Recommended test procedure

For each sentence below:

1. Launch `Talk.exe`.
2. Switch to **Transcribe** mode with `RightCtrl+1`.
3. Press `RightAlt` once to start recording.
4. Read exactly one target sentence.
5. Press `RightAlt` again to stop.
6. Record the final inserted/output text.
7. Compare it against the expected text exactly, including:
   - punctuation;
   - `Talk` capitalization;
   - `local first ASR`;
   - `Neuro Talk`;
   - `qwen3 asr flash`;
   - file path spelling.

For each failed sentence, repeat once more before classifying it as a real failure.

## 4. Must-pass core regression set

These eight sentences are the main human regression baseline for r26.

### P0-01 short greeting

- sample id: `short-search-001`
- say:
  - `你好呀`
- expected:
  - `你好呀`

### P0-02 mixed Chinese-English product phrase

- sample id: `mixed-english-001`
- say:
  - `打开 Talk 的 local first ASR 测试`
- expected:
  - `打开 Talk 的 local first ASR 测试`

### P0-03 Chinese-English-Japanese mixed page phrase

- sample id: `mixed-english-japanese-001`
- say:
  - `请帮我打开 Talk 的 local first ASR テスト 页面。`
- expected:
  - `请帮我打开 Talk 的 local first ASR テスト 页面。`

### P0-04 proper nouns and path preservation

- sample id: `proper-nouns-001`
- say:
  - `请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users\\Public\\Talk\\logs。`
- expected:
  - `请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users\\Public\\Talk\\logs。`

### P0-05 short punctuation sentence

- sample id: `punctuation-001`
- say:
  - `今天下午三点开会，请提醒我准备资料。`
- expected:
  - `今天下午三点开会，请提醒我准备资料。`

### P0-06 longform punctuation sentence

- sample id: `punctuation-longform-001`
- say:
  - `今天下午三点半我们先开项目例会，确认 Talk 的默认识别模型，然后把多语言测试结果同步给 Neuro 团队。`
- expected:
  - `今天下午三点半我们先开项目例会，确认 Talk 的默认识别模型，然后把多语言测试结果同步给 Neuro 团队。`

### P0-07 ordinary noisy sentence

- sample id: `natural-noise-001`
- say:
  - `这是一段普通语音输入测试，背景里可能会有一点噪音。`
- expected:
  - `这是一段普通语音输入测试，背景里可能会有一点噪音。`

### P0-08 office-noise product sentence

- sample id: `noise-realistic-001`
- say:
  - `现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。`
- expected:
  - `现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。`

## 5. r26 targeted watch list

These are the specific regression areas newly protected in r26.

The tester still reads the normal target sentence, but if the output contains the following wrong patterns, treat it as a high-priority failure.

### W1 local-first phonetic drift

Watch for any final text resembling:

- `套卡`
- `劳克风斯特`
- `localfoster`
- `rock foster`

Target sentence:

- `打开 Talk 的 local first ASR 测试`

### W2 keyboard-noise alias drift

Watch for any final text resembling:

- `键盘生`
- `套口`
- `多语音识别测试结果`

Target sentence:

- `现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。`

### W3 proper noun and path drift

Watch for any final text resembling:

- `CPA 的优秀`
- `SIPA 的 user`
- path truncation after `C:\\Users`
- loss of `Neuro Talk`
- loss of `qwen3 asr flash`

Target sentence:

- `请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users\\Public\\Talk\\logs。`

### W4 longform tail truncation

Watch for any final text ending early near:

- `然后把多语言测试结果同步给`

Target sentence:

- `今天下午三点半我们先开项目例会，确认 Talk 的默认识别模型，然后把多语言测试结果同步给 Neuro 团队。`

## 6. Stress pass

After the first quiet-environment pass, repeat these four sentences under slightly less ideal speaking conditions:

1. `打开 Talk 的 local first ASR 测试`
2. `请帮我打开 Talk 的 local first ASR テスト 页面。`
3. `请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users\\Public\\Talk\\logs。`
4. `现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。`

Recommended stress variants:

- speak slightly faster;
- reduce pause clarity once;
- add light fan or keyboard background noise once.

Do not add so much noise that the sentence becomes genuinely unintelligible.

## 7. Failure capture format

For every real failure, capture all of the following:

- build:
  - `talk-accuracy-20260729-r26`
- mode:
  - `transcribe`
- whether credentials were available:
  - `cloud-enabled` or `local-only`
- target sentence
- actual output text
- whether it failed on the first try only or both tries
- whether the environment was:
  - quiet;
  - light noise;
  - faster speech
- newest log json path from:
  - `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-accuracy-20260729-r26\.runtime\talk-desktop\logs`

If available, also keep the matching WAV artifact from:

- `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-accuracy-20260729-r26\.runtime\talk-desktop\audio`

## 8. Pass criteria

For the quiet-environment pass, r26 should be considered healthy when:

- all eight P0 sentences are exact matches; and
- none of the W1-W4 regression signatures appear.

For the stress pass, minor non-semantic punctuation drift can be noted separately, but any loss of:

- `Talk`
- `local first ASR`
- `Neuro Talk`
- `qwen3 asr flash`
- `C:\\Users\\Public\\Talk\\logs`
- `Neuro 团队`

should still be treated as a real regression.
