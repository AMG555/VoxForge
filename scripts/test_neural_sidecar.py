"""Unit tests for VoxForge neural sidecar DSP, cloning prep, caching, and routing."""

import base64
import io
import json
import sys
import wave
from pathlib import Path

import numpy as np
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import neural_sidecar as sidecar  # noqa: E402


def _make_tone(freq_hz: float, duration_s: float = 1.0, sr: int = 24000, amp: float = 0.5) -> np.ndarray:
    t = np.arange(int(sr * duration_s), dtype=np.float32) / float(sr)
    return (np.sin(2.0 * np.pi * freq_hz * t) * amp).astype(np.float32)


def _wav_bytes_from_f32(samples: np.ndarray, sr: int = 24000, channels: int = 1) -> bytes:
    buf = io.BytesIO()
    clipped = np.clip(samples, -1.0, 1.0)
    pcm16 = (clipped * 32767.0).astype(np.int16)
    with wave.open(buf, "wb") as wf:
        wf.setnchannels(channels)
        wf.setsampwidth(2)
        wf.setframerate(sr)
        wf.writeframes(pcm16.tobytes())
    return buf.getvalue()


def test_apply_highpass_80hz_attenuates_rumble_preserves_speech():
    sr = 24000
    rumble = _make_tone(20.0, 1.0, sr, amp=0.6)
    speech = _make_tone(400.0, 1.0, sr, amp=0.3)

    rumble_out = sidecar.apply_highpass_80hz(rumble, sample_rate=sr)
    speech_out = sidecar.apply_highpass_80hz(speech, sample_rate=sr)

    rumble_rms_in = float(np.sqrt(np.mean(rumble[sr // 4 :] ** 2)))
    rumble_rms_out = float(np.sqrt(np.mean(rumble_out[sr // 4 :] ** 2)))
    speech_rms_in = float(np.sqrt(np.mean(speech[sr // 4 :] ** 2)))
    speech_rms_out = float(np.sqrt(np.mean(speech_out[sr // 4 :] ** 2)))

    assert rumble_rms_out < rumble_rms_in * 0.25
    assert speech_rms_out > speech_rms_in * 0.90
    assert sidecar.apply_highpass_80hz(np.array([], dtype=np.float32), sr).size == 0


def test_sanitize_audio_replaces_nonfinite():
    arr = np.array([0.1, np.nan, np.inf, -np.inf, -0.2], dtype=np.float32)
    cleaned = sidecar.sanitize_audio(arr)
    assert np.all(np.isfinite(cleaned))
    assert np.allclose(cleaned, [0.1, 0.0, 0.0, 0.0, -0.2])


def test_downmix_to_mono_prevents_phase_cancellation():
    sig = _make_tone(220.0, 0.25, 24000, amp=0.5)
    inverted_stereo = np.stack([sig, -sig], axis=1)
    mono = sidecar.downmix_to_mono(inverted_stereo)
    mono_rms = float(np.sqrt(np.mean(mono ** 2)))
    sig_rms = float(np.sqrt(np.mean(sig ** 2)))
    # Naive averaging yields ~0 RMS; phase-safe downmix preserves signal energy
    assert mono_rms > sig_rms * 0.85


def test_remove_silence_safe_ladder_never_empties_quiet_clip():
    sr = 24000
    pad = np.zeros(sr // 2, dtype=np.float32)
    # Very quiet speech (~ -62 dBFS amplitude ~0.0008)
    quiet = _make_tone(300.0, 0.6, sr, amp=0.0012)
    audio = np.concatenate([pad, quiet, pad])
    trimmed = sidecar.remove_silence_safe(audio, sample_rate=sr)
    assert trimmed.size >= int(0.4 * sr)
    assert float(np.max(np.abs(trimmed))) > 0.0


def test_best_reference_window_drops_long_transcript_to_prevent_misalignment():
    sr = 24000
    # 24s clip (> 20s limit): quiet first 6s, strong speech in middle, quiet tail
    quiet = _make_tone(200.0, 6.0, sr, amp=0.02)
    strong = _make_tone(280.0, 12.0, sr, amp=0.35)
    tail = _make_tone(200.0, 6.0, sr, amp=0.02)
    long_clip = np.concatenate([quiet, strong, tail])

    windowed, adjusted_text = sidecar.prepare_reference_window(
        long_clip, sr, "This whole 24 second transcript must be dropped."
    )
    assert windowed.size <= int(15.2 * sr)
    assert windowed.size >= int(8.0 * sr)
    assert float(np.sqrt(np.mean(windowed ** 2))) > 0.15
    assert adjusted_text is None


def test_strip_script_markup_and_normalize_lang():
    cleaned = sidecar.strip_script_markup("Hello [pause 500ms] world [laugh] [[s iː|see]]!")
    assert "[pause" not in cleaned
    assert "[laugh]" not in cleaned
    assert "Hello" in cleaned and "world" in cleaned and "see!" in cleaned
    assert sidecar.strip_script_markup("  [sigh]  ") is None

    assert sidecar.normalize_language_code("arb") == "ar"
    assert sidecar.normalize_language_code("mal") == "ml"
    assert sidecar.normalize_language_code("ml-Latn") == "ml"
    assert sidecar.normalize_language_code("Portuguese (Brazil)") == "pt-BR"


def test_peak_limit_numpy_van_herk_enforces_ceiling_and_silence_floor():
    sr = 24000
    hot = _make_tone(350.0, 0.5, sr, amp=1.8)
    hot[120] = np.nan
    limited = sidecar.peak_limit_numpy(hot, sample_rate=sr, ceiling_db=-1.0)
    ceiling_lin = 10.0 ** (-1.0 / 20.0)
    assert np.all(np.isfinite(limited))
    assert float(np.max(np.abs(limited))) <= ceiling_lin + 1e-4

    # Silence below -50 dBFS must stay quiet (never amplified to static)
    silent = np.full(sr, 1e-5, dtype=np.float32)
    leveled = sidecar.level_speech_numpy(silent, sample_rate=sr)
    assert float(np.max(np.abs(leveled))) < 1e-3


def test_encode_wav_and_decode_ref_audio_roundtrip_all_bit_depths():
    sr = 24000
    tone = _make_tone(300.0, 0.5, sr, amp=0.4)
    for depth in (16, 24, 32):
        wav_bytes = sidecar.encode_wav(tone, sample_rate=sr, bit_depth=depth)
        assert wav_bytes[:4] == b"RIFF"
        b64 = base64.b64encode(wav_bytes).decode("ascii")
        samples, out_sr, tmp_path = sidecar.decode_ref_audio(b64)
        try:
            assert out_sr == sr
            assert samples.size > sr // 4
            assert np.all(np.isfinite(samples))
            assert tmp_path and Path(tmp_path).exists()
        finally:
            if tmp_path and Path(tmp_path).exists():
                Path(tmp_path).unlink(missing_ok=True)


def test_prompt_cache_lru_and_languages_catalog():
    cache = sidecar.VoiceClonePromptCache(max_entries=2)
    cache.put("k1", {"emb": 1})
    cache.put("k2", {"emb": 2})
    assert cache.get("k1") == {"emb": 1}  # k1 now MRU
    cache.put("k3", {"emb": 3})  # evicts k2
    assert cache.get("k2") is None
    assert cache.get("k1") == {"emb": 1}
    assert cache.get("k3") == {"emb": 3}

    langs = sidecar.load_languages_catalog()
    assert len(langs) >= 600


def test_synthesize_omnivoice_mock_passes_seed_cleaned_ref_and_precision(monkeypatch):
    captured = {}

    class DummyTensor:
        def __init__(self, arr):
            self._arr = arr

        def detach(self):
            return self

        def cpu(self):
            return self

        def numpy(self):
            return self._arr

    class DummyModel:
        def create_voice_clone_prompt(self, **kwargs):
            captured["prompt_kw"] = kwargs
            return {"cached_prompt": True}

        def generate(self, **kwargs):
            captured["gen_kw"] = kwargs
            return DummyTensor(_make_tone(320.0, 0.4, 24000, amp=0.35))

    monkeypatch.setattr(sidecar, "ACTIVE_BACKEND", "omnivoice")
    monkeypatch.setattr(sidecar, "_tts_model", DummyModel())
    sidecar._PROMPT_CACHE.clear()

    ref_wav = _wav_bytes_from_f32(_make_tone(240.0, 1.0, 24000, amp=0.04), sr=24000)
    req = {
        "text": "നമസ്കാരം സുഹൃത്തേ",
        "language": "ml-IN",
        "reference_audio": base64.b64encode(ref_wav).decode("ascii"),
        "reference_transcript": "  [pause]  ",
        "speed": 1.05,
        "num_step": 32,
        "precision": 24,
        "seed": 424242,
        "mastering": True,
    }
    wav_bytes, sr = sidecar.synthesize(req)
    assert sr == 24000
    assert wav_bytes[:4] == b"RIFF"
    # Unpack bits-per-sample from WAV fmt chunk (bytes 34..36)
    bits_per_sample = int.from_bytes(wav_bytes[34:36], "little")
    assert bits_per_sample == 24
    # Empty-after-markup-strip transcript must become None, not ""
    assert captured["gen_kw"].get("ref_text") is None
    assert captured["gen_kw"].get("language") == "ml-IN"
    assert "voice_clone_prompt" in captured["gen_kw"] or "ref_audio" in captured["gen_kw"]
