"""
VoxForge Neural TTS & Voice Cloning Sidecar Server

Provides an HTTP inference microservice bridging VoxForge's Rust API server
to state-of-the-art local neural TTS models with true zero-shot voice cloning,
reference audio windowing, prompt caching, deterministic seeding, and broadcast mastering.

Supported Neural Backends (tried in priority order):
  1. OmniVoice (k2-fsa/OmniVoice) — Default engine.
     - Massively multilingual: 646 languages.
     - Diffusion flow-matching with configurable refinement steps (8-64, default 32).
     - Automatic reference transcription via faster-whisper / openai-whisper.
     - Prompt caching + automatic best-window slicing for long references.
  2. CosyVoice 3 / 2 (FunAudioLLM/Fun-CosyVoice3-0.5B)
  3. F5-TTS (SWivid/F5-TTS)
  4. Kokoro-82M (hexgrad/Kokoro-82M)

Usage:
  python scripts/neural_sidecar.py                     # Auto-detect, port 9000
  python scripts/neural_sidecar.py --backend omnivoice # Force OmniVoice (646 langs)
  python scripts/neural_sidecar.py --backend cosyvoice # Force CosyVoice 3
  python scripts/neural_sidecar.py --backend f5tts     # Force F5-TTS
  python scripts/neural_sidecar.py --backend kokoro    # Force Kokoro-82M
  python scripts/neural_sidecar.py --port 9000         # Custom port

Environment variables:
  VOXFORGE_SIDECAR_PORT    Default port (9000)
  VOXFORGE_SIDECAR_DEVICE  Force device: cuda, mps, cpu (default: auto)
  OMNIVOICE_CHECKPOINT     HuggingFace ID or local path (default: k2-fsa/OmniVoice)
  COSYVOICE_MODEL_DIR      Local CosyVoice 3 model dir
  F5TTS_MODEL              F5-TTS model checkpoint
"""

import argparse
import base64
import gc
import hashlib
import http.server
import io
import json
import logging
import math
import os
import random
import re
import struct
import sys
import tempfile
import threading
import wave
from collections import OrderedDict
from http.server import BaseHTTPRequestHandler
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

import numpy as np

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
)
log = logging.getLogger("voxforge-sidecar")

_SCRIPT_DIR = Path(__file__).resolve().parent
_REPO_ROOT = _SCRIPT_DIR.parent

CLONE_REF_TEXT_MAX_SECONDS = 20.0
TARGET_BEST_WINDOW_SECONDS = 15.0
SILENCE_FLOOR_DBFS = -50.0
SILENCE_FLOOR_AMP = 10.0 ** (SILENCE_FLOOR_DBFS / 20.0)
REF_TARGET_RMS = 0.1
MAX_PROMPT_CACHE_ENTRIES = 8

ACTIVE_BACKEND: Optional[str] = None
_tts_model: Any = None
_asr_model: Any = None
_lock = threading.Lock()


class VoiceClonePromptCache:
    """Bounded LRU cache for encoded reference voice clone prompts."""

    def __init__(self, max_entries: int = MAX_PROMPT_CACHE_ENTRIES):
        self.max_entries = max(1, int(max_entries))
        self._entries: "OrderedDict[str, Any]" = OrderedDict()

    @staticmethod
    def compute_key(
        backend: str,
        audio_bytes: bytes,
        ref_text: Optional[str],
        preprocess_tag: str = "v1",
    ) -> str:
        hasher = hashlib.sha256()
        hasher.update(backend.encode("utf-8"))
        hasher.update(b":")
        hasher.update(audio_bytes)
        hasher.update(b":")
        hasher.update((ref_text or "").encode("utf-8"))
        hasher.update(b":")
        hasher.update(preprocess_tag.encode("utf-8"))
        return hasher.hexdigest()

    def get(self, key: str) -> Any:
        if key not in self._entries:
            return None
        self._entries.move_to_end(key)
        return self._entries[key]

    def put(self, key: str, value: Any) -> None:
        if key in self._entries:
            self._entries.move_to_end(key)
        self._entries[key] = value
        while len(self._entries) > self.max_entries:
            self._entries.popitem(last=False)

    def clear(self) -> None:
        self._entries.clear()


_PROMPT_CACHE = VoiceClonePromptCache(max_entries=MAX_PROMPT_CACHE_ENTRIES)

_MACROLANGUAGE_MAP: Dict[str, str] = {
    "arb": "ar",
    "arz": "ar",
    "acm": "ar",
    "apc": "ar",
    "ary": "ar",
    "ajp": "ar",
    "aeb": "ar",
    "apd": "ar",
    "cmn": "zh",
    "zho": "zh",
    "yue": "zh",
    "wuu": "zh",
    "mal": "ml",
    "hin": "hi",
    "tam": "ta",
    "tel": "te",
    "kan": "kn",
    "ben": "bn",
    "urd": "ur",
    "npi": "ne",
    "nep": "ne",
    "jpn": "ja",
    "kor": "ko",
    "fra": "fr",
    "fre": "fr",
    "deu": "de",
    "ger": "de",
    "spa": "es",
    "por": "pt",
    "rus": "ru",
    "ita": "it",
    "nld": "nl",
    "dut": "nl",
    "tur": "tr",
    "vie": "vi",
    "ind": "id",
    "tha": "th",
    "ell": "el",
    "gre": "el",
    "heb": "he",
    "ukr": "uk",
    "pol": "pl",
    "ces": "cs",
    "cze": "cs",
    "swe": "sv",
    "ron": "ro",
    "rum": "ro",
    "hun": "hu",
    "fas": "fa",
    "per": "fa",
    "mar": "mr",
    "guj": "gu",
    "pan": "pa",
    "ori": "or",
    "asm": "as",
    "sin": "si",
    "khm": "km",
    "lao": "lo",
    "mya": "my",
    "bur": "my",
    "amh": "am",
    "kat": "ka",
    "geo": "ka",
    "hye": "hy",
    "arm": "hy",
}

_VARIANT_NAME_MAP: Dict[str, str] = {
    "portuguese (brazil)": "pt-BR",
    "brazilian portuguese": "pt-BR",
    "portuguese (portugal)": "pt-PT",
    "european portuguese": "pt-PT",
    "chinese (simplified)": "zh-CN",
    "chinese (traditional)": "zh-TW",
    "ml-latn": "ml",
    "hi-latn": "hi",
    "ta-latn": "ta",
}

_MARKUP_TAG_RE = re.compile(r"\[[^\[\]]*\]")
_PHONETIC_PAIR_RE = re.compile(r"\[\[[^|\]]*\|([^\]]+)\]\]")


def normalize_language_code(lang: Optional[str]) -> str:
    """Normalize BCP-47, ISO 639-3 macrolanguage codes, and display names."""
    if not lang:
        return "en-US"
    raw = str(lang).strip()
    if not raw or raw.upper() == "AUTO":
        return "en-US"

    lower = raw.lower().replace("_", "-")
    if lower in _VARIANT_NAME_MAP:
        return _VARIANT_NAME_MAP[lower]
    if lower in _MACROLANGUAGE_MAP:
        return _MACROLANGUAGE_MAP[lower]

    parts = [p for p in lower.split("-") if p]
    if len(parts) >= 2:
        base = _MACROLANGUAGE_MAP.get(parts[0], parts[0])
        region = parts[1]
        if region == "latn":
            return base
        if region in ("hans", "cn", "sg"):
            return f"{base}-CN"
        if region in ("hant", "tw", "hk"):
            return f"{base}-TW"
        if len(region) == 2:
            return f"{base}-{region.upper()}"
        return f"{base}-{region}"
    return _MACROLANGUAGE_MAP.get(parts[0], parts[0])


def strip_script_markup(text: Optional[str]) -> Optional[str]:
    """Strip inline cue/pause tags and bracketed phonetic notation from reference text."""
    if not text:
        return None
    cleaned = _PHONETIC_PAIR_RE.sub(r"\1", str(text))
    cleaned = _MARKUP_TAG_RE.sub(" ", cleaned)
    cleaned = re.sub(r"\s+", " ", cleaned).strip()
    return cleaned or None


def detect_device() -> str:
    override = os.environ.get("VOXFORGE_SIDECAR_DEVICE", "").strip().lower()
    if override:
        return override
    try:
        import torch

        if torch.cuda.is_available():
            return "cuda"
        if hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
            return "mps"
    except ImportError:
        pass
    return "cpu"


def _set_deterministic_seed(seed: Optional[int]) -> None:
    if seed is None:
        return
    try:
        s = int(seed)
    except (TypeError, ValueError):
        return
    random.seed(s)
    np.random.seed(s % (2**32 - 1))
    try:
        import torch

        torch.manual_seed(s)
        if torch.cuda.is_available():
            torch.cuda.manual_seed_all(s)
    except ImportError:
        pass


def _load_whisper_asr():
    """Lazy load faster-whisper or standard whisper for automatic reference audio transcription."""
    global _asr_model
    if _asr_model is not None:
        return _asr_model

    device = detect_device()
    try:
        from faster_whisper import WhisperModel

        compute_type = "float16" if device == "cuda" else "int8"
        log.info(
            "Loading faster-whisper (base) on %s (%s) for reference audio ASR...",
            device,
            compute_type,
        )
        _asr_model = (
            "faster_whisper",
            WhisperModel("base", device=device, compute_type=compute_type),
        )
        log.info("faster-whisper ASR loaded successfully")
        return _asr_model
    except Exception as exc:
        log.debug("faster-whisper unavailable (%s), trying openai-whisper...", exc)

    try:
        import whisper

        log.info("Loading openai-whisper (base) on %s for reference audio ASR...", device)
        _asr_model = ("whisper", whisper.load_model("base", device=device))
        log.info("openai-whisper ASR loaded successfully")
        return _asr_model
    except Exception as exc:
        log.warning("No local Whisper ASR available (%s)", exc)
        return None


def _release_implicit_asr() -> None:
    """Release lazily loaded ASR model to free GPU/CPU RAM before heavy TTS decoding."""
    global _asr_model
    if _asr_model is None:
        return
    _asr_model = None
    gc.collect()
    try:
        import torch

        if torch.cuda.is_available():
            torch.cuda.empty_cache()
    except ImportError:
        pass


def transcribe_reference_audio(
    wav_path: str,
    language: Optional[str] = None,
    release_after: bool = False,
) -> Optional[str]:
    """Transcribe a reference WAV file so zero-shot diffusion has aligned conditioning text."""
    asr = _load_whisper_asr()
    if asr is None:
        return None
    kind, model = asr
    lang_hint = None
    if language and language.upper() != "AUTO":
        lang_hint = normalize_language_code(language).split("-")[0]
    try:
        if kind == "faster_whisper":
            kw: Dict[str, Any] = {"beam_size": 3, "vad_filter": True}
            if lang_hint:
                kw["language"] = lang_hint
            segments, _ = model.transcribe(wav_path, **kw)
            text = " ".join(seg.text.strip() for seg in segments if seg.text).strip()
            return text or None

        kw = {}
        if lang_hint:
            kw["language"] = lang_hint
        res = model.transcribe(wav_path, **kw)
        text = res.get("text", "").strip()
        return text or None
    except Exception as exc:
        log.warning("Reference transcription failed: %s", exc)
        return None
    finally:
        if release_after:
            _release_implicit_asr()


# ---------------------------------------------------------------------------
# Backend loaders
# ---------------------------------------------------------------------------


def _load_omnivoice(device: str) -> bool:
    """Load k2-fsa/OmniVoice (646 languages, diffusion flow matching)."""
    global _tts_model, ACTIVE_BACKEND
    try:
        log.info("Attempting to load k2-fsa/OmniVoice on %s...", device)
        from omnivoice import OmniVoice

        checkpoint = os.environ.get("OMNIVOICE_CHECKPOINT", "k2-fsa/OmniVoice")
        _tts_model = OmniVoice.from_pretrained(checkpoint, device_map=device)
        ACTIVE_BACKEND = "omnivoice"
        log.info("k2-fsa/OmniVoice loaded successfully (646 languages supported)")
        return True
    except Exception as exc:
        log.debug("OmniVoice not available: %s", exc)
        return False


def _load_cosyvoice3(device: str) -> bool:
    """Load Fun-CosyVoice3-0.5B or CosyVoice2."""
    global _tts_model, ACTIVE_BACKEND
    try:
        log.info("Attempting to load CosyVoice 3 / 2 on %s...", device)
        from cosyvoice.cli.cosyvoice import AutoModel, CosyVoice2

        model_dir = os.environ.get(
            "COSYVOICE_MODEL_DIR",
            os.path.join(
                os.path.expanduser("~"),
                ".cache",
                "modelscope",
                "hub",
                "FunAudioLLM",
                "Fun-CosyVoice3-0.5B",
            ),
        )
        if os.path.exists(model_dir):
            try:
                _tts_model = AutoModel(model_dir=model_dir)
                ACTIVE_BACKEND = "cosyvoice3"
                log.info("Fun-CosyVoice3 loaded from %s", model_dir)
                return True
            except Exception as inner_exc:
                log.debug("CosyVoice3 AutoModel load failed (%s), trying CosyVoice2...", inner_exc)
                _tts_model = CosyVoice2(model_dir=model_dir)
                ACTIVE_BACKEND = "cosyvoice"
                log.info("CosyVoice 2 loaded from %s", model_dir)
                return True
    except Exception as exc:
        log.debug("CosyVoice unavailable: %s", exc)
    return False


def _load_f5tts(device: str) -> bool:
    """Load F5-TTS flow matching model."""
    global _tts_model, ACTIVE_BACKEND
    try:
        from f5_tts.api import F5TTS

        checkpoint = os.environ.get("F5TTS_MODEL")
        log.info("Loading F5-TTS on %s...", device)
        _tts_model = F5TTS(model_type="F5TTS", device=device, ckpt_file=checkpoint or "")
        ACTIVE_BACKEND = "f5tts"
        log.info("F5-TTS loaded (%s)", device)
        return True
    except Exception as exc:
        log.debug("F5-TTS unavailable: %s", exc)
        return False


def _load_kokoro(device: str) -> bool:
    """Load Kokoro-82M lightweight neural model."""
    global _tts_model, ACTIVE_BACKEND
    try:
        from kokoro import KPipeline

        log.info("Loading Kokoro-82M on %s...", device)
        _tts_model = KPipeline(lang_code="a", device=device)
        ACTIVE_BACKEND = "kokoro"
        log.info("Kokoro-82M loaded (%s)", device)
        return True
    except Exception as exc:
        log.debug("Kokoro unavailable: %s", exc)
        return False


def load_backend(preferred: Optional[str] = None) -> bool:
    device = detect_device()
    order = [preferred] if preferred else ["omnivoice", "cosyvoice3", "cosyvoice", "f5tts", "kokoro"]
    loaders = {
        "omnivoice": _load_omnivoice,
        "cosyvoice3": _load_cosyvoice3,
        "cosyvoice": _load_cosyvoice3,
        "f5tts": _load_f5tts,
        "kokoro": _load_kokoro,
    }
    for name in order:
        loader = loaders.get(name)
        if loader and loader(device):
            return True

    log.warning("NO local neural TTS backend found in Python environment!")
    log.info("Install one of the following for local zero-shot neural voice cloning:")
    log.info("  1. pip install omnivoice")
    log.info("  2. pip install f5-tts")
    log.info("  3. pip install git+https://github.com/FunAudioLLM/CosyVoice")
    return False


# ---------------------------------------------------------------------------
# Studio Audio Safety & Broadcast Mastering DSP (Vectorized, Zero-Loop)
# ---------------------------------------------------------------------------


def sanitize_audio(samples: np.ndarray) -> np.ndarray:
    """Convert audio array to contiguous float32 with NaN/Inf safely zeroed."""
    arr = np.asarray(samples, dtype=np.float32)
    if arr.size == 0:
        return np.zeros(0, dtype=np.float32)
    return np.ascontiguousarray(np.nan_to_num(arr, nan=0.0, posinf=0.0, neginf=0.0), dtype=np.float32)


def downmix_to_mono(samples: np.ndarray) -> np.ndarray:
    """Downmix multi-channel audio without phase-cancelling inverted dual mics."""
    arr = sanitize_audio(samples)
    if arr.ndim == 1:
        return arr
    if arr.ndim > 2:
        arr = arr.reshape(arr.shape[0], -1)

    # Normalize layout to (num_samples, num_channels)
    if arr.shape[0] < arr.shape[1] and arr.shape[0] <= 8:
        arr = arr.T

    if arr.shape[1] == 1:
        return np.ascontiguousarray(arr[:, 0], dtype=np.float32)

    mixed = np.mean(arr, axis=1, dtype=np.float32)
    mixed_rms = float(np.sqrt(np.mean(mixed * mixed)))
    chan_rms = np.sqrt(np.mean(arr * arr, axis=0))
    loudest_idx = int(np.argmax(chan_rms))
    loudest_rms = float(chan_rms[loudest_idx])
    if loudest_rms > 1e-6 and mixed_rms < 0.5 * loudest_rms:
        return np.ascontiguousarray(arr[:, loudest_idx], dtype=np.float32)
    return np.ascontiguousarray(mixed, dtype=np.float32)


def apply_highpass_80hz(samples: np.ndarray, sample_rate: int = 24000, cutoff_hz: float = 80.0) -> np.ndarray:
    """2nd-order Butterworth 80Hz high-pass filter removing desk rumble and DC offset."""
    x = sanitize_audio(samples)
    if x.size < 4 or sample_rate <= 0 or cutoff_hz <= 0.0 or cutoff_hz >= 0.45 * sample_rate:
        return x

    x = x - np.mean(x, dtype=np.float32)
    try:
        from scipy.signal import butter, sosfilt

        sos = butter(2, cutoff_hz, btype="highpass", fs=float(sample_rate), output="sos")
        return sanitize_audio(sosfilt(sos, x))
    except Exception:
        w0 = 2.0 * math.pi * cutoff_hz / float(sample_rate)
        cos_w0 = math.cos(w0)
        alpha = math.sin(w0) / (2.0 * 0.70710678)
        a0 = 1.0 + alpha
        b0 = ((1.0 + cos_w0) * 0.5) / a0
        b1 = -(1.0 + cos_w0) / a0
        b2 = b0
        a1 = (-2.0 * cos_w0) / a0
        a2 = (1.0 - alpha) / a0

        y = np.empty_like(x)
        x1 = x2 = y1 = y2 = 0.0
        for i in range(x.shape[0]):
            x0 = float(x[i])
            y0 = b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2
            y[i] = y0
            x2, x1 = x1, x0
            y2, y1 = y1, y0
        return sanitize_audio(y)


def _fade_out_tail(samples: np.ndarray, sample_rate: int, fade_ms: float = 25.0) -> np.ndarray:
    if samples.size == 0 or sample_rate <= 0:
        return samples
    fade_len = min(samples.size, max(1, int(round(sample_rate * fade_ms / 1000.0))))
    if fade_len <= 1:
        return samples
    out = samples.copy()
    ramp = 0.5 * (1.0 + np.cos(np.linspace(0.0, np.pi, fade_len, dtype=np.float32)))
    out[-fade_len:] *= ramp
    return out


def remove_silence_safe(
    samples: np.ndarray,
    sample_rate: int = 24000,
    thresholds_dbfs: Tuple[float, ...] = (-50.0, -60.0, -70.0),
) -> np.ndarray:
    """Trim leading/trailing silence with a progressive dBFS ladder; never empties quiet speech."""
    x = sanitize_audio(samples)
    if x.size == 0 or sample_rate <= 0:
        return x

    hop = max(1, int(round(sample_rate * 0.02)))  # 20ms frames
    pad_samples = max(1, int(round(sample_rate * 0.06)))  # 60ms breathing room
    n_frames = max(1, x.shape[0] // hop)
    trimmed_len = n_frames * hop
    if trimmed_len == 0:
        return x

    frame_rms = np.sqrt(np.mean(x[:trimmed_len].reshape(n_frames, hop) ** 2, axis=1))
    min_keep = min(x.shape[0], int(round(sample_rate * 0.15)))

    for thresh_db in thresholds_dbfs:
        thresh_amp = 10.0 ** (thresh_db / 20.0)
        active = np.flatnonzero(frame_rms >= thresh_amp)
        if active.size == 0:
            continue
        start = max(0, int(active[0]) * hop - pad_samples)
        end = min(x.shape[0], int(active[-1] + 1) * hop + pad_samples)
        if end - start >= min_keep:
            return _fade_out_tail(np.ascontiguousarray(x[start:end]), sample_rate)

    return x


def trim_passage_to_pauses(
    samples: np.ndarray,
    sample_rate: int,
    start_idx: int,
    target_len: int,
) -> Tuple[int, int]:
    """Refine slice boundaries toward nearby low-energy pause frames to avoid mid-word cuts."""
    n = samples.shape[0]
    hop = max(1, int(round(sample_rate * 0.04)))
    search_radius = int(round(sample_rate * 0.75))

    def _nearest_pause(center: int) -> int:
        lo = max(0, center - search_radius)
        hi = min(n, center + search_radius)
        seg = samples[lo:hi]
        nf = seg.shape[0] // hop
        if nf < 2:
            return max(0, min(n, center))
        rms = np.sqrt(np.mean(seg[: nf * hop].reshape(nf, hop) ** 2, axis=1))
        min_frame = int(np.argmin(rms))
        return max(0, min(n, lo + min_frame * hop + hop // 2))

    s = _nearest_pause(start_idx)
    e = _nearest_pause(min(n, start_idx + target_len))
    if e <= s + int(round(sample_rate * 4.0)):
        return max(0, start_idx), min(n, start_idx + target_len)
    return s, e


def prepare_reference_window(
    samples: np.ndarray,
    sample_rate: int,
    ref_transcript: Optional[str],
) -> Tuple[np.ndarray, Optional[str]]:
    """Select clean <=15s reference window on >20s clips and prevent misaligned transcripts."""
    cleaned_text = strip_script_markup(ref_transcript)
    x = remove_silence_safe(samples, sample_rate=sample_rate)
    if x.size == 0 or sample_rate <= 0:
        return x, cleaned_text

    duration_s = float(x.shape[0]) / float(sample_rate)
    if duration_s <= CLONE_REF_TEXT_MAX_SECONDS:
        return x, cleaned_text

    # Clip exceeds max aligned transcript length: select best ~15s passage and drop whole-clip transcript
    target_len = min(x.shape[0], int(round(TARGET_BEST_WINDOW_SECONDS * sample_rate)))
    hop = max(1, int(round(2.0 * sample_rate)))
    best_score = -1.0
    best_start = 0

    for start in range(0, max(1, x.shape[0] - target_len + 1), hop):
        seg = x[start : start + target_len]
        rms = float(np.sqrt(np.mean(seg * seg)))
        if rms < SILENCE_FLOOR_AMP:
            continue
        frame_step = max(1, int(round(0.04 * sample_rate)))
        nf = seg.shape[0] // frame_step
        if nf > 0:
            frames = np.sqrt(np.mean(seg[: nf * frame_step].reshape(nf, frame_step) ** 2, axis=1))
            voiced_ratio = float(np.mean(frames >= SILENCE_FLOOR_AMP))
            p95 = float(np.percentile(frames, 95))
        else:
            voiced_ratio = 1.0
            p95 = rms
        score = voiced_ratio * rms * (1.0 + min(2.0, p95 / max(rms, 1e-6)))
        if score > best_score:
            best_score = score
            best_start = start

    s, e = trim_passage_to_pauses(x, sample_rate, best_start, target_len)
    best_slice = remove_silence_safe(x[s:e], sample_rate=sample_rate)
    return best_slice, None


def _box_mean(values: np.ndarray, width: int) -> np.ndarray:
    n = values.shape[0]
    if n == 0 or width <= 1:
        return values.astype(np.float32, copy=True)
    half = width // 2
    padded = np.pad(values.astype(np.float64), (half, width - 1 - half), mode="edge")
    csum = np.empty(padded.shape[0] + 1, dtype=np.float64)
    csum[0] = 0.0
    np.cumsum(padded, out=csum[1:])
    return ((csum[width : width + n] - csum[:n]) / float(width)).astype(np.float32)


def _sliding_window_min(values: np.ndarray, width: int) -> np.ndarray:
    """O(n) exact centered sliding-window minimum via van Herk / Gil-Werman block scan."""
    n = values.shape[0]
    if n == 0 or width <= 1:
        return values.astype(np.float32, copy=True)
    k = min(width, n)
    if k <= 1:
        return values.astype(np.float32, copy=True)

    pad_left = k // 2
    pad_right = k - 1 - pad_left
    padded = np.pad(values.astype(np.float32), (pad_left, pad_right), mode="edge")
    m = padded.shape[0]
    rem = (-m) % k
    if rem:
        padded = np.pad(padded, (0, rem), mode="edge")
    blocks = padded.reshape(-1, k)

    # Running suffix min within each block (forward across axis 1 = minimum.accumulate)
    prefix = np.minimum.accumulate(blocks, axis=1)
    suffix = np.minimum.accumulate(blocks[:, ::-1], axis=1)[:, ::-1]

    flat_out = np.minimum(suffix[:-1, :].ravel() if blocks.shape[0] > 1 else suffix.ravel()[:n], prefix[1:, :].ravel() if blocks.shape[0] > 1 else prefix.ravel()[:n])
    if blocks.shape[0] > 1:
        starts = np.arange(n)
        blk_idx = starts // k
        offset = starts % k
        res = np.where(offset == 0, prefix[blk_idx, k - 1], np.minimum(suffix[blk_idx, offset], prefix[blk_idx + 1, offset - 1]))
        return res.astype(np.float32)
    return flat_out[:n].astype(np.float32)


def level_speech_numpy(
    audio: np.ndarray,
    sample_rate: int = 24000,
    target_dbfs: float = -20.0,
    gate_dbfs: float = -45.0,
    max_gain_db: float = 16.0,
    min_gain_db: float = -12.0,
) -> np.ndarray:
    """Slow phrase-level speech leveler respecting the -50 dBFS silence floor."""
    x = sanitize_audio(audio)
    if x.size < 64 or sample_rate <= 0:
        return x

    peak = float(np.max(np.abs(x)))
    if peak < SILENCE_FLOOR_AMP:
        return x

    frame_len = max(1, int(round(sample_rate * 0.04)))
    n_frames = x.shape[0] // frame_len
    if n_frames < 4:
        return x

    frames = x[: n_frames * frame_len].reshape(n_frames, frame_len)
    frame_rms = np.sqrt(np.mean(frames * frames, axis=1) + 1e-12)
    gate_amp = max(SILENCE_FLOOR_AMP, 10.0 ** (gate_dbfs / 20.0))
    speech_mask = frame_rms >= gate_amp
    if not np.any(speech_mask):
        return x

    smoothed_rms = np.interp(
        np.arange(n_frames),
        np.flatnonzero(speech_mask),
        frame_rms[speech_mask],
    )
    win_frames = max(1, int(round(0.400 / 0.04)))
    smoothed_rms = _box_mean(smoothed_rms.astype(np.float32), win_frames)

    target_amp = 10.0 ** (target_dbfs / 20.0)
    min_gain = 10.0 ** (min_gain_db / 20.0)
    max_gain = 10.0 ** (max_gain_db / 20.0)
    frame_gain = np.clip(target_amp / np.maximum(smoothed_rms, 1e-6), min_gain, max_gain)
    frame_gain = _box_mean(frame_gain, max(1, int(round(0.200 / 0.04))))

    sample_idx = np.arange(x.shape[0], dtype=np.float32) / float(frame_len) - 0.5
    gain_curve = np.interp(sample_idx, np.arange(n_frames, dtype=np.float32), frame_gain).astype(np.float32)
    return sanitize_audio(x * gain_curve)


def peak_limit_numpy(
    audio: np.ndarray,
    sample_rate: int = 24000,
    ceiling_db: float = -1.0,
    window_ms: float = 5.0,
) -> np.ndarray:
    """O(n) look-ahead peak limiter using van Herk sliding minimum and hard safety clamp."""
    x = sanitize_audio(audio)
    if x.size == 0 or sample_rate <= 0:
        return x

    ceiling = 10.0 ** (ceiling_db / 20.0)
    abs_x = np.abs(x)
    if float(np.max(abs_x)) <= ceiling:
        return x

    req_gain = np.minimum(1.0, ceiling / np.maximum(abs_x, 1e-9)).astype(np.float32)
    width = max(3, int(round(sample_rate * window_ms / 1000.0)) | 1)
    min_gain = _sliding_window_min(req_gain, width)
    smooth_gain = _box_mean(min_gain, max(1, int(round(sample_rate * 0.002))))
    gain = np.minimum(min_gain, smooth_gain)

    limited = np.clip(x * gain, -ceiling, ceiling)
    return sanitize_audio(limited)


def apply_broadcast_mastering(samples: np.ndarray, sample_rate: int = 24000) -> np.ndarray:
    """Apply zero-reverb broadcast polish: 80Hz HPF, mild warmth, speech leveler, look-ahead limiter."""
    x = sanitize_audio(samples)
    if x.size == 0:
        return x
    x = apply_highpass_80hz(x, sample_rate=sample_rate, cutoff_hz=80.0)
    # Subtle analog warmth saturation (20% blend, no reverb)
    warm = np.tanh(x * 1.15) / math.tanh(1.15)
    x = 0.80 * x + 0.20 * warm
    x = level_speech_numpy(x, sample_rate=sample_rate, target_dbfs=-20.0)
    x = peak_limit_numpy(x, sample_rate=sample_rate, ceiling_db=-1.0)
    return _fade_out_tail(x, sample_rate=sample_rate, fade_ms=15.0)


def encode_wav(samples_f32: np.ndarray, sample_rate: int = 24000, bit_depth: int = 16) -> bytes:
    """Vectorized RIFF WAV encoder supporting 16-bit PCM, 24-bit PCM, and 32-bit float."""
    arr = np.clip(sanitize_audio(samples_f32), -1.0, 1.0)
    buf = io.BytesIO()
    n = int(arr.size)

    if bit_depth == 32:
        data_bytes = arr.astype("<f4").tobytes()
        block_align = 4
        byte_rate = sample_rate * block_align
        fmt_chunk = struct.pack("<4sIHHIIHH", b"fmt ", 16, 3, 1, sample_rate, byte_rate, block_align, 32)
        riff_size = 4 + len(fmt_chunk) + 8 + len(data_bytes)
        buf.write(struct.pack("<4sI4s", b"RIFF", riff_size, b"WAVE"))
        buf.write(fmt_chunk)
        buf.write(struct.pack("<4sI", b"data", len(data_bytes)))
        buf.write(data_bytes)
        return buf.getvalue()

    if bit_depth == 24:
        scaled = np.clip(np.round(arr * 8388607.0), -8388608, 8388607).astype("<i4")
        data_bytes = scaled.view("<u1").reshape(n, 4)[:, :3].tobytes() if n > 0 else b""
        block_align = 3
        byte_rate = sample_rate * block_align
        fmt_chunk = struct.pack("<4sIHHIIHH", b"fmt ", 16, 1, 1, sample_rate, byte_rate, block_align, 24)
        riff_size = 4 + len(fmt_chunk) + 8 + len(data_bytes)
        buf.write(struct.pack("<4sI4s", b"RIFF", riff_size, b"WAVE"))
        buf.write(fmt_chunk)
        buf.write(struct.pack("<4sI", b"data", len(data_bytes)))
        buf.write(data_bytes)
        return buf.getvalue()

    with wave.open(buf, "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(sample_rate)
        pcm = np.clip(np.round(arr * 32767.0), -32768, 32767).astype("<i2")
        wf.writeframes(pcm.tobytes())
    return buf.getvalue()


def _decode_wav_bytes(raw: bytes) -> Optional[Tuple[np.ndarray, int]]:
    """Decode 8/16/24/32-bit PCM or 32/64-bit IEEE float RIFF WAV into float32 mono."""
    if len(raw) < 12 or raw[:4] != b"RIFF" or raw[8:12] != b"WAVE":
        return None

    offset = 12
    fmt_tag: Optional[int] = None
    channels = 1
    sample_rate = 24000
    bits_per_sample = 16
    data_payload: Optional[bytes] = None

    while offset + 8 <= len(raw):
        chunk_id = raw[offset : offset + 4]
        chunk_size = struct.unpack("<I", raw[offset + 4 : offset + 8])[0]
        chunk_start = offset + 8
        chunk_end = min(len(raw), chunk_start + chunk_size)
        if chunk_id == b"fmt " and chunk_size >= 16:
            fmt_tag, channels, sample_rate, _, _, bits_per_sample = struct.unpack(
                "<HHIIHH", raw[chunk_start : chunk_start + 16]
            )
            if fmt_tag == 0xFFFE and chunk_size >= 40:
                subformat = struct.unpack("<H", raw[chunk_start + 24 : chunk_start + 26])[0]
                fmt_tag = subformat
        elif chunk_id == b"data":
            data_payload = raw[chunk_start:chunk_end]
        offset = chunk_start + chunk_size + (chunk_size & 1)

    if data_payload is None or fmt_tag is None:
        return None

    if fmt_tag == 3 and bits_per_sample == 32:
        arr = np.frombuffer(data_payload[: len(data_payload) - (len(data_payload) % 4)], dtype="<f4").astype(np.float32)
    elif fmt_tag == 3 and bits_per_sample == 64:
        arr = np.frombuffer(data_payload[: len(data_payload) - (len(data_payload) % 8)], dtype="<f8").astype(np.float32)
    elif fmt_tag == 1 and bits_per_sample == 16:
        arr = np.frombuffer(data_payload[: len(data_payload) - (len(data_payload) % 2)], dtype="<i2").astype(np.float32) / 32768.0
    elif fmt_tag == 1 and bits_per_sample == 24:
        usable = len(data_payload) - (len(data_payload) % 3)
        if usable == 0:
            return np.zeros(0, dtype=np.float32), sample_rate
        raw3 = np.frombuffer(data_payload[:usable], dtype=np.uint8).reshape(-1, 3)
        padded = np.zeros((raw3.shape[0], 4), dtype=np.uint8)
        padded[:, 1:] = raw3
        arr = (padded.view("<i4").reshape(-1).astype(np.float32)) / 2147483648.0
    elif fmt_tag == 1 and bits_per_sample == 32:
        arr = np.frombuffer(data_payload[: len(data_payload) - (len(data_payload) % 4)], dtype="<i4").astype(np.float32) / 2147483648.0
    elif fmt_tag == 1 and bits_per_sample == 8:
        arr = (np.frombuffer(data_payload, dtype=np.uint8).astype(np.float32) - 128.0) / 128.0
    else:
        return None

    if channels > 1:
        usable = len(arr) - (len(arr) % channels)
        arr = arr[:usable].reshape(-1, channels)
        arr = downmix_to_mono(arr)
    else:
        arr = sanitize_audio(arr)

    return arr, int(sample_rate)


def decode_ref_audio(b64_or_path: str) -> Tuple[np.ndarray, int, Optional[str]]:
    """Decode reference audio base64/path into clean mono float32 samples + temp WAV path."""
    if not b64_or_path or not isinstance(b64_or_path, str):
        return np.zeros(0, dtype=np.float32), 24000, None

    try:
        samples: Optional[np.ndarray] = None
        sr = 24000

        if os.path.exists(b64_or_path):
            raw = Path(b64_or_path).read_bytes()
        else:
            raw_b64 = b64_or_path.split(",", 1)[1] if "," in b64_or_path else b64_or_path
            raw = base64.b64decode(raw_b64, validate=False)

        decoded = _decode_wav_bytes(raw)
        if decoded is not None:
            samples, sr = decoded
        else:
            try:
                import torchaudio

                wav_tensor, sr_t = torchaudio.load(io.BytesIO(raw))
                samples = downmix_to_mono(wav_tensor.detach().cpu().numpy())
                sr = int(sr_t)
            except Exception:
                from scipy.io import wavfile

                sr_s, data = wavfile.read(io.BytesIO(raw))
                if data.dtype == np.int16:
                    arr = data.astype(np.float32) / 32768.0
                elif data.dtype == np.int32:
                    arr = data.astype(np.float32) / 2147483648.0
                else:
                    arr = data.astype(np.float32)
                samples = downmix_to_mono(arr)
                sr = int(sr_s)

        if samples is None or samples.size == 0:
            raise ValueError("Decoded reference audio contained zero samples")

        samples = apply_highpass_80hz(samples, sample_rate=sr, cutoff_hz=80.0)
        tmp = tempfile.NamedTemporaryFile(suffix=".wav", delete=False)
        try:
            tmp.write(encode_wav(samples, sample_rate=sr, bit_depth=16))
            tmp.flush()
            return samples, sr, tmp.name
        finally:
            tmp.close()

    except Exception as exc:
        log.warning("Reference audio decode failed: %s", exc)
        return np.zeros(0, dtype=np.float32), 24000, None


def _scale_reference_amplitude(samples: np.ndarray) -> Tuple[np.ndarray, float]:
    """Lift quiet reference RMS toward 0.1 before speaker encoding, preserving dynamic headroom."""
    x = sanitize_audio(samples)
    if x.size == 0:
        return x, 0.0
    rms = float(np.sqrt(np.mean(x * x)))
    if rms < SILENCE_FLOOR_AMP:
        return x, rms
    scale = min(12.0, REF_TARGET_RMS / max(rms, 1e-6))
    if abs(scale - 1.0) < 0.05:
        return x, rms
    peak = float(np.max(np.abs(x)))
    if peak * scale > 0.98:
        scale = 0.98 / max(peak, 1e-6)
    return sanitize_audio(x * scale), rms


def _extract_audio_array(res: Any) -> np.ndarray:
    obj = res[0] if isinstance(res, (list, tuple)) else res
    if isinstance(obj, dict) and "tts_speech" in obj:
        obj = obj["tts_speech"]
    if hasattr(obj, "detach"):
        obj = obj.detach()
    if hasattr(obj, "cpu"):
        obj = obj.cpu()
    if hasattr(obj, "numpy"):
        obj = obj.numpy()
    return sanitize_audio(np.asarray(obj).reshape(-1))


# ---------------------------------------------------------------------------
# Synthesis Dispatch & Catalog
# ---------------------------------------------------------------------------


def load_languages_catalog() -> List[Dict[str, Any]]:
    """Load the 646-language catalog cleanly from bundled project metadata files."""
    tsv_path = _REPO_ROOT / "data" / "lang_id_name_map.tsv"
    if tsv_path.exists():
        langs: List[Dict[str, Any]] = []
        for line in tsv_path.read_text(encoding="utf-8").splitlines()[1:]:
            parts = line.split("\t")
            if len(parts) >= 3:
                langs.append({"id": parts[0], "name": parts[1], "iso": parts[2]})
        if langs:
            return langs

    json_path = _REPO_ROOT / "ui" / "src" / "constants" / "languages646.json"
    if json_path.exists():
        raw_list = json.loads(json_path.read_text(encoding="utf-8"))
        if isinstance(raw_list, list):
            return raw_list

    return []


def synthesize(body: Dict[str, Any]) -> Tuple[bytes, int]:
    text = str(body.get("text", "")).strip()
    if not text:
        raise ValueError("Missing 'text' field")

    language = normalize_language_code(body.get("language", "en-US"))
    ref_audio_raw = body.get("reference_audio")
    raw_transcript = body.get("reference_transcript") or body.get("ref_text")
    speed = float(body.get("speed", 1.0))
    refinement_steps = max(4, min(64, int(body.get("num_step", body.get("steps", 32)))))
    precision = int(body.get("precision", body.get("bit_depth", 16)))
    if precision not in (16, 24, 32):
        precision = 16
    apply_mastering = bool(body.get("mastering", True))
    seed = body.get("seed")
    instruct = body.get("instruct")

    _set_deterministic_seed(seed)

    ref_samples, ref_sr, tmp_path = (
        decode_ref_audio(ref_audio_raw) if ref_audio_raw else (np.zeros(0, dtype=np.float32), 24000, None)
    )
    cleaned_transcript: Optional[str] = strip_script_markup(raw_transcript)

    try:
        if tmp_path and ref_samples.size > 0:
            win_samples, adjusted_transcript = prepare_reference_window(
                ref_samples, ref_sr, cleaned_transcript
            )
            scaled_samples, _ = _scale_reference_amplitude(win_samples)
            cleaned_wav_bytes = encode_wav(scaled_samples, sample_rate=ref_sr, bit_depth=16)
            Path(tmp_path).write_bytes(cleaned_wav_bytes)
            cleaned_transcript = adjusted_transcript

            # Auto-transcribe if transcript missing or invalidated after slicing (>20s clips)
            if not cleaned_transcript and ACTIVE_BACKEND in ("omnivoice", "cosyvoice3", "cosyvoice", "f5tts"):
                transcribed = transcribe_reference_audio(
                    tmp_path, language=language, release_after=True
                )
                cleaned_transcript = strip_script_markup(transcribed)
        else:
            cleaned_wav_bytes = b""

        if ACTIVE_BACKEND == "omnivoice":
            kw: Dict[str, Any] = {
                "text": text,
                "language": language,
                "num_step": refinement_steps,
                "speed": speed,
            }
            if instruct:
                kw["instruct"] = str(instruct)

            if tmp_path:
                prompt_key = VoiceClonePromptCache.compute_key(
                    "omnivoice", cleaned_wav_bytes, cleaned_transcript
                )
                cached_prompt = _PROMPT_CACHE.get(prompt_key)
                if cached_prompt is None and hasattr(_tts_model, "create_voice_clone_prompt"):
                    prompt_kw: Dict[str, Any] = {"ref_audio": tmp_path}
                    if cleaned_transcript is not None:
                        prompt_kw["ref_text"] = cleaned_transcript
                    cached_prompt = _tts_model.create_voice_clone_prompt(**prompt_kw)
                    _PROMPT_CACHE.put(prompt_key, cached_prompt)

                if cached_prompt is not None:
                    kw["voice_clone_prompt"] = cached_prompt
                    kw["ref_text"] = cleaned_transcript
                else:
                    kw["ref_audio"] = tmp_path
                    kw["ref_text"] = cleaned_transcript

            res = _tts_model.generate(**kw)
            audio_f32 = _extract_audio_array(res)
            sr_out = int(getattr(_tts_model, "sampling_rate", 24000))

        elif ACTIVE_BACKEND in ("cosyvoice3", "cosyvoice"):
            lang_prefix = language.split("-")[0]
            if tmp_path and cleaned_transcript:
                gen = _tts_model.inference_zero_shot(
                    text, cleaned_transcript, tmp_path, speed=speed, stream=False
                )
            elif tmp_path:
                gen = _tts_model.inference_cross_lingual(
                    f"<|{lang_prefix}|>{text}", tmp_path, speed=speed, stream=False
                )
            else:
                spks = _tts_model.list_available_spks()
                spk = spks[0] if spks else "中文女"
                gen = _tts_model.inference_sft(text, spk, speed=speed, stream=False)
            parts = [_extract_audio_array(item) for item in list(gen)]
            parts = [p for p in parts if p.size > 0]
            audio_f32 = np.concatenate(parts) if parts else np.zeros(0, dtype=np.float32)
            sr_out = 24000

        elif ACTIVE_BACKEND == "f5tts":
            kw = {"gen_text": text, "speed": speed, "nfe_step": refinement_steps}
            if tmp_path:
                kw["ref_audio"] = tmp_path
                kw["ref_text"] = cleaned_transcript or ""
            wav, sr, _ = _tts_model.infer(**kw)
            audio_f32 = _extract_audio_array(wav)
            sr_out = int(sr)

        elif ACTIVE_BACKEND == "kokoro":
            parts = []
            for _, _, audio in _tts_model(text, speed=speed):
                arr = _extract_audio_array(audio)
                if arr.size > 0:
                    parts.append(arr)
            audio_f32 = np.concatenate(parts) if parts else np.zeros(0, dtype=np.float32)
            sr_out = 24000

        else:
            raise RuntimeError(f"No active neural backend loaded (ACTIVE_BACKEND={ACTIVE_BACKEND})")

    finally:
        if tmp_path and tmp_path != ref_audio_raw and os.path.exists(tmp_path):
            try:
                os.unlink(tmp_path)
            except OSError:
                pass

    audio_f32 = sanitize_audio(audio_f32)
    if apply_mastering:
        audio_f32 = apply_broadcast_mastering(audio_f32, sample_rate=sr_out)
    else:
        audio_f32 = peak_limit_numpy(audio_f32, sample_rate=sr_out, ceiling_db=-0.5)

    wav_bytes = encode_wav(audio_f32, sample_rate=sr_out, bit_depth=precision)
    return wav_bytes, sr_out


# ---------------------------------------------------------------------------
# HTTP Server Handler
# ---------------------------------------------------------------------------


class SidecarHandler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        log.debug(fmt, *args)

    def _send_json(self, code: int, data: Any):
        payload = json.dumps(data).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def _send_wav(self, wav_bytes: bytes):
        self.send_response(200)
        self.send_header("Content-Type", "audio/wav")
        self.send_header("Content-Length", str(len(wav_bytes)))
        self.end_headers()
        self.wfile.write(wav_bytes)

    def do_GET(self):
        path = self.path.split("?")[0].rstrip("/")
        if path in ("", "/health", "/healthz"):
            self._send_json(
                200,
                {
                    "status": "ok",
                    "backend": ACTIVE_BACKEND or "none",
                    "device": detect_device(),
                    "languages_supported": 646 if ACTIVE_BACKEND == "omnivoice" else len(load_languages_catalog()),
                    "precision_supported": [16, 24, 32],
                    "mastering_available": True,
                },
            )
        elif path == "/languages":
            langs = load_languages_catalog()
            self._send_json(200, {"count": len(langs), "languages": langs})
        else:
            self._send_json(404, {"error": "not found"})

    def do_POST(self):
        path = self.path.split("?")[0].rstrip("/")
        length = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(length) if length else b"{}"
        try:
            body = json.loads(raw.decode("utf-8"))
        except Exception as exc:
            self._send_json(400, {"error": f"Invalid JSON: {exc}"})
            return

        if path == "/transcribe":
            audio_raw = body.get("audio_base64") or body.get("reference_audio") or body.get("audio")
            if not audio_raw:
                self._send_json(400, {"error": "Missing 'audio_base64' field"})
                return
            samples, _, tmp_path = decode_ref_audio(audio_raw)
            try:
                if not tmp_path or samples.size == 0:
                    self._send_json(400, {"error": "Failed to decode audio payload"})
                    return
                transcript = transcribe_reference_audio(
                    tmp_path,
                    language=body.get("language"),
                    release_after=bool(body.get("release_after", False)),
                )
                self._send_json(200, {"text": transcript or ""})
            finally:
                if tmp_path and os.path.exists(tmp_path):
                    try:
                        os.unlink(tmp_path)
                    except OSError:
                        pass
            return

        text = str(body.get("text", "")).strip()
        if not text:
            self._send_json(400, {"error": "Missing 'text' field"})
            return

        with _lock:
            try:
                wav_bytes, _ = synthesize(body)
                self._send_wav(wav_bytes)
            except Exception as exc:
                log.exception("Synthesis error")
                self._send_json(500, {"error": str(exc)})


def main():
    parser = argparse.ArgumentParser(description="VoxForge Neural Sidecar")
    parser.add_argument(
        "--backend",
        choices=["omnivoice", "cosyvoice", "cosyvoice3", "f5tts", "kokoro"],
        default=None,
    )
    parser.add_argument(
        "--port",
        type=int,
        default=int(os.environ.get("VOXFORGE_SIDECAR_PORT", 9000)),
    )
    args = parser.parse_args()

    load_backend(preferred=args.backend)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", args.port), SidecarHandler)
    log.info("VoxForge Neural Sidecar listening on http://127.0.0.1:%d", args.port)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        log.info("Shutting down sidecar...")
        server.shutdown()


if __name__ == "__main__":
    main()
