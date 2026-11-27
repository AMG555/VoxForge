import React, { useState, useRef, useEffect } from 'react';
import {
  Play,
  Pause,
  Plus,
  Trash2,
  Download,
  Sparkles,
  RotateCcw,
  Clock,
  Layers,
  FileAudio,
  Square,
} from 'lucide-react';
import { Voice } from '../../types';
import { api, isAbortError } from '../../services/api';

export interface TimelineClip {
  id: string;
  startSec: number;
  durationSec: number;
  text: string;
  voiceId: string;
  audioBlob?: Blob;
  audioUrl?: string;
  isSynthesized: boolean;
}

export interface TimelineTrack {
  id: string;
  name: string;
  voiceId: string;
  color: string;
  volume: number; // 0 to 1
  pan: number; // -1 to 1
  isMuted: boolean;
  isSoloed: boolean;
  clips: TimelineClip[];
}

interface MultiTrackTimelineProps {
  voices: Voice[];
}

const DEFAULT_TRACK_COLORS = ['#F59E0B', '#38BDF8', '#A855F7', '#10B981', '#EC4899'];

function audioBufferToWav(buffer: AudioBuffer): Blob {
  const numOfChan = buffer.numberOfChannels;
  const length = buffer.length * numOfChan * 2 + 44;
  const outBuffer = new ArrayBuffer(length);
  const view = new DataView(outBuffer);
  const channels: Float32Array[] = [];
  const sampleRate = buffer.sampleRate;
  let offset = 0;
  let pos = 0;

  function setUint16(data: number) {
    view.setUint16(pos, data, true);
    pos += 2;
  }
  function setUint32(data: number) {
    view.setUint32(pos, data, true);
    pos += 4;
  }

  // write WAVE header
  setUint32(0x46464952); // "RIFF"
  setUint32(length - 8); // file length - 8
  setUint32(0x45564157); // "WAVE"

  setUint32(0x20746d66); // "fmt " chunk
  setUint32(16); // length = 16
  setUint16(1); // PCM (uncompressed)
  setUint16(numOfChan);
  setUint32(sampleRate);
  setUint32(sampleRate * 2 * numOfChan); // avg. bytes/sec
  setUint16(numOfChan * 2); // block-align
  setUint16(16); // 16-bit precision

  setUint32(0x61746164); // "data" - chunk
  setUint32(length - pos - 4); // chunk length

  for (let i = 0; i < numOfChan; i++) {
    channels.push(buffer.getChannelData(i));
  }

  while (offset < buffer.length) {
    for (let i = 0; i < numOfChan; i++) {
      let sample = Math.max(-1, Math.min(1, channels[i][offset]));
      sample = (sample < 0 ? sample * 32768 : sample * 32767) | 0;
      view.setInt16(pos, sample, true);
      pos += 2;
    }
    offset++;
  }

  return new Blob([outBuffer], { type: 'audio/wav' });
}

function pickReadyDefaultVoice(
  voices: Voice[],
  preferredId: string,
  indexFallback = 0
): string {
  const exact = voices.find((v) => v.id === preferredId);
  if (exact) return exact.id;
  const edgeVoices = voices.filter((v) => v.engine_id === 'edge-tts');
  if (edgeVoices.length > 0) {
    return edgeVoices[indexFallback % edgeVoices.length].id;
  }
  const usable = voices.filter((v) => v.engine_id !== 'mock-tts');
  return usable[indexFallback % Math.max(1, usable.length)]?.id || preferredId;
}

export const MultiTrackTimeline: React.FC<MultiTrackTimelineProps> = ({ voices }) => {
  const defaultHostVoice = pickReadyDefaultVoice(voices, 'en-US-AriaNeural', 0);
  const defaultGuestVoice = pickReadyDefaultVoice(voices, 'en-US-GuyNeural', 1);

  const [tracks, setTracks] = useState<TimelineTrack[]>([
    {
      id: 'track-1',
      name: 'Host (Julian Drake)',
      voiceId: defaultHostVoice,
      color: '#F59E0B',
      volume: 0.9,
      pan: -0.15,
      isMuted: false,
      isSoloed: false,
      clips: [
        {
          id: 'clip-1',
          startSec: 0.5,
          durationSec: 4.5,
          text: 'Welcome to VoxForge Studio. Today we examine neural synthesis and high-performance audio graphs.',
          voiceId: defaultHostVoice,
          isSynthesized: false,
        },
        {
          id: 'clip-3',
          startSec: 9.5,
          durationSec: 5.0,
          text: 'Notice how the multi-engine router seamlessly distributes workloads across local and remote clusters.',
          voiceId: defaultHostVoice,
          isSynthesized: false,
        },
      ],
    },
    {
      id: 'track-2',
      name: 'Guest (Dr. Aris Vance)',
      voiceId: defaultGuestVoice,
      color: '#38BDF8',
      volume: 0.85,
      pan: 0.2,
      isMuted: false,
      isSoloed: false,
      clips: [
        {
          id: 'clip-2',
          startSec: 5.2,
          durationSec: 4.0,
          text: 'Indeed, Julian. Zero-latency streaming completely changes the ergonomics of interactive AI agents.',
          voiceId: defaultGuestVoice,
          isSynthesized: false,
        },
      ],
    },
  ]);

  const usableVoices = voices.filter((v) => v.engine_id !== 'mock-tts');
  const [isPlaying, setIsPlaying] = useState<boolean>(false);
  const [currentTimeSec, setCurrentTimeSec] = useState<number>(0);
  const [zoomPxPerSec, setZoomPxPerSec] = useState<number>(45);
  const [isRenderingMaster, setIsRenderingMaster] = useState<boolean>(false);
  const renderAbortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    return () => {
      renderAbortRef.current?.abort();
    };
  }, []);

  const handleCancelRender = () => {
    if (renderAbortRef.current) {
      renderAbortRef.current.abort();
      renderAbortRef.current = null;
    }
  };
  const [masterAudioUrl, setMasterAudioUrl] = useState<string | null>(null);
  const [isMasterDirty, setIsMasterDirty] = useState<boolean>(true);
  const [isAuditioningMaster, setIsAuditioningMaster] = useState<boolean>(false);
  const [timelineStatus, setTimelineStatus] = useState<{ text: string; isError?: boolean } | null>(null);
  const [playingClipId, setPlayingClipId] = useState<string | null>(null);
  const [newClipText, setNewClipText] = useState<string>('');
  const [activeTrackIdForAdd, setActiveTrackIdForAdd] = useState<string>(tracks[0]?.id || '');
  const [draggedClip, setDraggedClip] = useState<{
    trackId: string;
    clipId: string;
    startX: number;
    initialStartSec: number;
  } | null>(null);

  const playheadIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const masterAudioRef = useRef<HTMLAudioElement | null>(null);
  const clipAudioRef = useRef<HTMLAudioElement | null>(null);

  const totalDurationSec = Math.max(
    20,
    ...tracks.flatMap((t) => t.clips.map((c) => c.startSec + c.durationSec + 2))
  );

  // Dragging clips horizontally along the timeline
  useEffect(() => {
    if (!draggedClip) return;

    const handleMouseMove = (e: MouseEvent) => {
      const deltaSec = (e.clientX - draggedClip.startX) / zoomPxPerSec;
      const newStartSec = Math.max(0, Math.round((draggedClip.initialStartSec + deltaSec) * 10) / 10);
      setTracks((prev) =>
        prev.map((t) =>
          t.id === draggedClip.trackId
            ? {
                ...t,
                clips: t.clips.map((c) =>
                  c.id === draggedClip.clipId ? { ...c, startSec: newStartSec } : c
                ),
              }
            : t
        )
      );
    };

    const handleMouseUp = () => {
      setDraggedClip(null);
      setIsMasterDirty(true);
    };

    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);
    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [draggedClip, zoomPxPerSec]);

  // Synchronized Playhead Scrubber during Transport Play
  useEffect(() => {
    if (isPlaying) {
      playheadIntervalRef.current = setInterval(() => {
        if (masterAudioRef.current && !masterAudioRef.current.paused) {
          setCurrentTimeSec(masterAudioRef.current.currentTime);
        } else {
          setCurrentTimeSec((prev) => {
            if (prev >= totalDurationSec) {
              setIsPlaying(false);
              return 0;
            }
            return prev + 0.1;
          });
        }
      }, 80);
    } else {
      if (masterAudioRef.current && !masterAudioRef.current.paused) {
        masterAudioRef.current.pause();
        setIsAuditioningMaster(false);
      }
      if (playheadIntervalRef.current) clearInterval(playheadIntervalRef.current);
    }
    return () => {
      if (playheadIntervalRef.current) clearInterval(playheadIntervalRef.current);
    };
  }, [isPlaying, totalDurationSec]);

  // Tear down stale audio elements only when URL actually changed (do not pause newly-started audio for the same URL)
  useEffect(() => {
    if (
      masterAudioRef.current &&
      (!masterAudioUrl || masterAudioRef.current.src !== masterAudioUrl)
    ) {
      masterAudioRef.current.pause();
      masterAudioRef.current = null;
      setIsAuditioningMaster(false);
    }
    return () => {
      if (masterAudioUrl) URL.revokeObjectURL(masterAudioUrl);
    };
  }, [masterAudioUrl]);

  useEffect(() => {
    return () => {
      if (clipAudioRef.current) {
        clipAudioRef.current.pause();
        clipAudioRef.current = null;
      }
    };
  }, []);

  const startMasterPlaybackFrom = (url: string, startSec: number) => {
    if (masterAudioRef.current) {
      masterAudioRef.current.pause();
    }
    const audio = new Audio(url);
    audio.currentTime = Math.max(0, startSec);
    audio.onended = () => {
      setIsAuditioningMaster(false);
      setIsPlaying(false);
      setCurrentTimeSec(0);
    };
    audio.onpause = () => {
      if (masterAudioRef.current === audio) {
        setIsAuditioningMaster(false);
        setIsPlaying(false);
      }
    };
    audio.ontimeupdate = () => {
      setCurrentTimeSec(audio.currentTime);
    };
    masterAudioRef.current = audio;
    setIsAuditioningMaster(true);
    setIsPlaying(true);
    audio.play().catch((e) => setTimelineStatus({ text: `Playback error: ${e.message}`, isError: true }));
  };

  const handleToggleTransportPlay = async () => {
    if (isPlaying) {
      if (masterAudioRef.current && !masterAudioRef.current.paused) {
        masterAudioRef.current.pause();
      }
      setIsPlaying(false);
      return;
    }
    if (masterAudioUrl && !isMasterDirty) {
      startMasterPlaybackFrom(masterAudioUrl, currentTimeSec);
      return;
    }
    const url = await handleRenderMaster();
    if (url) {
      startMasterPlaybackFrom(url, currentTimeSec);
    }
  };

  const toggleAuditionMaster = () => {
    if (!masterAudioUrl) return;
    if (isAuditioningMaster && masterAudioRef.current) {
      masterAudioRef.current.pause();
      setIsAuditioningMaster(false);
      setIsPlaying(false);
    } else {
      startMasterPlaybackFrom(masterAudioUrl, 0);
    }
  };

  const handleAuditionSingleClip = async (trackVoiceId: string, clip: TimelineClip) => {
    if (clipAudioRef.current) {
      clipAudioRef.current.pause();
      clipAudioRef.current = null;
      if (playingClipId === clip.id) {
        setPlayingClipId(null);
        return;
      }
    }
    setPlayingClipId(clip.id);
    setTimelineStatus({ text: `Synthesizing clip (${trackVoiceId})...` });
    try {
      let blob = clip.audioBlob;
      if (!blob) {
        blob = await api.synthesizeDirect({
          input: clip.text,
          voice: clip.voiceId || trackVoiceId,
          speed: 1.0,
        });
      }
      const clipUrl = URL.createObjectURL(blob);
      const audio = new Audio(clipUrl);
      clipAudioRef.current = audio;
      audio.onended = () => {
        setPlayingClipId(null);
        URL.revokeObjectURL(clipUrl);
      };
      audio.play().catch(console.error);
      setTimelineStatus(null);
    } catch (err: any) {
      setPlayingClipId(null);
      setTimelineStatus({ text: err?.message || 'Clip synthesis failed', isError: true });
    }
  };

  const handleAddTrack = () => {
    const newIdx = tracks.length + 1;
    const color = DEFAULT_TRACK_COLORS[tracks.length % DEFAULT_TRACK_COLORS.length];
    const pool = usableVoices.length > 0 ? usableVoices : voices;
    const newTrack: TimelineTrack = {
      id: `track-${Date.now()}`,
      name: `Speaker ${newIdx}`,
      voiceId: pickReadyDefaultVoice(pool, 'en-US-JennyNeural', tracks.length),
      color,
      volume: 0.85,
      pan: 0.0,
      isMuted: false,
      isSoloed: false,
      clips: [],
    };
    setTracks((prev) => [...prev, newTrack]);
    setIsMasterDirty(true);
  };

  const handleDeleteTrack = (trackId: string) => {
    setTracks((prev) => {
      const next = prev.filter((t) => t.id !== trackId);
      if (activeTrackIdForAdd === trackId) {
        setActiveTrackIdForAdd(next[0]?.id || '');
      }
      return next;
    });
    setIsMasterDirty(true);
  };

  const handleAddClip = (trackId: string) => {
    if (!newClipText.trim()) return;
    const targetTrack = tracks.find((t) => t.id === trackId) || tracks[0];
    if (!targetTrack) return;

    const estimatedDuration = Math.max(2.0, newClipText.split(/\s+/).length / 2.8);
    const latestEnd = Math.max(
      currentTimeSec,
      ...targetTrack.clips.map((c) => c.startSec + c.durationSec)
    );

    const newClip: TimelineClip = {
      id: `clip-${Date.now()}`,
      startSec: Math.round(latestEnd * 10) / 10,
      durationSec: Math.round(estimatedDuration * 10) / 10,
      text: newClipText.trim(),
      voiceId: targetTrack.voiceId,
      isSynthesized: false,
    };

    setTracks((prev) =>
      prev.map((t) => (t.id === targetTrack.id ? { ...t, clips: [...t.clips, newClip] } : t))
    );
    setNewClipText('');
    setIsMasterDirty(true);
  };

  const handleDeleteClip = (trackId: string, clipId: string) => {
    setTracks((prev) =>
      prev.map((t) =>
        t.id === trackId ? { ...t, clips: t.clips.filter((c) => c.id !== clipId) } : t
      )
    );
    setIsMasterDirty(true);
  };

  const handleRenderMaster = async (): Promise<string | null> => {
    renderAbortRef.current?.abort();
    const controller = new AbortController();
    renderAbortRef.current = controller;
    setTimelineStatus(null);
    try {
      setIsRenderingMaster(true);
      const anySoloed = tracks.some((t) => t.isSoloed);
      const allActiveClips = tracks
        .filter((t) => (anySoloed ? t.isSoloed && !t.isMuted : !t.isMuted))
        .flatMap((t) =>
          t.clips.map((c) => ({
            ...c,
            trackId: t.id,
            trackVolume: t.volume,
            trackPan: t.pan,
            trackVoiceId: t.voiceId,
          }))
        )
        .sort((a, b) => a.startSec - b.startSec);

      if (allActiveClips.length === 0) {
        setTimelineStatus({ text: 'No active unmuted clips on timeline to render.', isError: true });
        return null;
      }

      const sampleRate = 24000;
      const decodeCtx = new (window.OfflineAudioContext ||
        (window as unknown as { webkitOfflineAudioContext: typeof OfflineAudioContext })
          .webkitOfflineAudioContext)(1, sampleRate, sampleRate);

      const decodedClips: Array<{
        clip: typeof allActiveClips[0];
        buffer: AudioBuffer;
        blob: Blob;
      }> = [];

      for (let i = 0; i < allActiveClips.length; i++) {
        const clip = allActiveClips[i];
        if (controller.signal.aborted) return null;
        setTimelineStatus({
          text: `Synthesizing clip ${i + 1}/${allActiveClips.length}...`,
        });
        let blob = clip.audioBlob;
        if (!blob) {
          blob = await api.synthesizeDirect({
            input: clip.text,
            voice: clip.voiceId || clip.trackVoiceId,
            speed: 1.0,
            signal: controller.signal,
          });
        }
        if (controller.signal.aborted) return null;
        const arrayBuf = await blob.arrayBuffer();
        const audioBuffer = await decodeCtx.decodeAudioData(arrayBuf.slice(0));
        decodedClips.push({ clip, buffer: audioBuffer, blob });
      }

      const actualEndTime = Math.max(
        ...decodedClips.map((d) => d.clip.startSec + d.buffer.duration)
      );
      const mixCtx = new (window.OfflineAudioContext ||
        (window as unknown as { webkitOfflineAudioContext: typeof OfflineAudioContext })
          .webkitOfflineAudioContext)(
        2,
        Math.max(sampleRate, Math.ceil((actualEndTime + 0.5) * sampleRate)),
        sampleRate
      );

      for (const item of decodedClips) {
        const source = mixCtx.createBufferSource();
        source.buffer = item.buffer;

        const gainNode = mixCtx.createGain();
        gainNode.gain.value = item.clip.trackVolume ?? 1.0;

        let lastNode: AudioNode = gainNode;
        if (typeof mixCtx.createStereoPanner === 'function' && item.clip.trackPan !== 0) {
          const panner = mixCtx.createStereoPanner();
          panner.pan.value = Math.max(-1, Math.min(1, item.clip.trackPan));
          gainNode.connect(panner);
          lastNode = panner;
        }

        source.connect(gainNode);
        lastNode.connect(mixCtx.destination);
        source.start(item.clip.startSec);
      }

      const renderedBuffer = await mixCtx.startRendering();
      const wavBlob = audioBufferToWav(renderedBuffer);

      // Update clip durations & cached audio blobs cleanly
      const cacheMap = new Map(
        decodedClips.map((d) => [d.clip.id, { blob: d.blob, dur: d.buffer.duration }])
      );
      setTracks((prev) =>
        prev.map((t) => ({
          ...t,
          clips: t.clips.map((c) => {
            const hit = cacheMap.get(c.id);
            return hit
              ? {
                  ...c,
                  durationSec: Math.round(hit.dur * 10) / 10,
                  audioBlob: hit.blob,
                  isSynthesized: true,
                }
              : c;
          }),
        }))
      );

      if (masterAudioUrl) URL.revokeObjectURL(masterAudioUrl);
      const url = URL.createObjectURL(wavBlob);
      setMasterAudioUrl(url);
      setIsMasterDirty(false);
      setTimelineStatus({ text: `Master mix rendered (${actualEndTime.toFixed(1)}s)` });
      return url;
    } catch (err: any) {
      if (isAbortError(err)) {
        setTimelineStatus({ text: 'Timeline render cancelled' });
        return null;
      }
      console.error('Master multi-track mix error:', err);
      setTimelineStatus({
        text: err?.message || 'Timeline synthesis failed — check selected track voice.',
        isError: true,
      });
      return null;
    } finally {
      if (renderAbortRef.current === controller) {
        renderAbortRef.current = null;
      }
      setIsRenderingMaster(false);
    }
  };

  return (
    <div className="flex-1 flex flex-col h-full bg-[#0B0E14] text-white select-none overflow-hidden">
      {/* Master Transport Toolbar */}
      <div className="h-14 border-b border-[#242E3D] bg-[#121820] px-4 flex items-center justify-between shrink-0">
        <div className="flex items-center space-x-4">
          <div className="flex items-center space-x-2">
            <div className="p-1.5 rounded-lg bg-amber-500/10 border border-amber-500/20 text-amber-400">
              <Layers className="w-4 h-4" />
            </div>
            <div>
              <h2 className="text-xs font-bold text-white flex items-center space-x-2">
                <span>STORIES TIMELINE (DAW)</span>
                <span className="px-1.5 py-0.5 rounded text-[9px] font-mono bg-amber-500/20 text-amber-300 border border-amber-500/30">
                  MULTI-TRACK
                </span>
              </h2>
              <p className="text-[10px] text-[#94A3B8] font-mono">
                Multi-speaker dialogue editor, podcast sequencer & master audio bounce
              </p>
            </div>
          </div>

          {/* Transport Controls */}
          <div className="flex items-center space-x-2 bg-[#0B0E14] px-3 py-1.5 rounded-lg border border-[#242E3D]">
            <button
              onClick={handleToggleTransportPlay}
              disabled={isRenderingMaster}
              className="p-1.5 rounded bg-amber-500 hover:bg-amber-400 text-black transition-colors disabled:opacity-50"
              title={isPlaying ? 'Pause' : 'Play Multi-Track Mix'}
            >
              {isPlaying ? <Pause className="w-3.5 h-3.5 fill-current" /> : <Play className="w-3.5 h-3.5 fill-current" />}
            </button>
            <button
              onClick={() => {
                setIsPlaying(false);
                setCurrentTimeSec(0);
              }}
              className="p-1.5 rounded text-[#94A3B8] hover:text-white hover:bg-[#1A222D] transition-colors"
              title="Return to Zero"
            >
              <RotateCcw className="w-3.5 h-3.5" />
            </button>

            <div className="flex items-center space-x-1.5 pl-2 font-mono text-xs text-amber-400 font-bold border-l border-[#242E3D]">
              <Clock className="w-3.5 h-3.5 text-[#64748B]" />
              <span>{Math.floor(currentTimeSec / 60).toString().padStart(2, '0')}:{(currentTimeSec % 60).toFixed(1).padStart(4, '0')}</span>
              <span className="text-[#64748B]">/</span>
              <span className="text-[#94A3B8]">{Math.floor(totalDurationSec / 60).toString().padStart(2, '0')}:{(totalDurationSec % 60).toFixed(0).padStart(2, '0')}</span>
            </div>
          </div>
        </div>

        {/* Right Action Bar */}
        <div className="flex items-center space-x-3">
          <div className="flex items-center space-x-1 text-xs font-mono text-[#94A3B8]">
            <span>Zoom:</span>
            <button
              onClick={() => setZoomPxPerSec((z) => Math.max(25, z - 10))}
              className="px-2 py-0.5 rounded bg-[#1A222D] hover:bg-[#242E3D] text-white border border-[#242E3D]"
            >
              -
            </button>
            <span className="w-8 text-center">{zoomPxPerSec}px</span>
            <button
              onClick={() => setZoomPxPerSec((z) => Math.min(90, z + 10))}
              className="px-2 py-0.5 rounded bg-[#1A222D] hover:bg-[#242E3D] text-white border border-[#242E3D]"
            >
              +
            </button>
          </div>

          <button
            onClick={handleAddTrack}
            className="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-[#1A222D] hover:bg-[#242E3D] text-white border border-[#242E3D] text-xs font-mono transition-colors"
          >
            <Plus className="w-3.5 h-3.5 text-amber-400" />
            <span>Add Track</span>
          </button>

          <button
            onClick={() => void handleRenderMaster()}
            disabled={isRenderingMaster}
            className="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-amber-500 hover:bg-amber-400 text-black font-semibold text-xs font-mono transition-colors shadow-sm disabled:opacity-50"
          >
            <Sparkles className="w-3.5 h-3.5" />
            <span>{isRenderingMaster ? 'Bouncing Master...' : 'Export Master WAV'}</span>
          </button>
          {isRenderingMaster && (
            <button
              type="button"
              onClick={handleCancelRender}
              className="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-red-500/15 hover:bg-red-500/25 text-red-300 border border-red-500/40 font-semibold text-xs font-mono transition-colors"
            >
              <Square className="w-3 h-3 fill-current" />
              <span>Cancel</span>
            </button>
          )}

          {masterAudioUrl && (
            <div className="flex items-center space-x-1.5">
              <button
                type="button"
                onClick={toggleAuditionMaster}
                className="flex items-center space-x-1 px-2.5 py-1.5 rounded-lg bg-emerald-500/20 text-emerald-300 border border-emerald-500/40 hover:bg-emerald-500/30 text-xs font-mono transition-colors"
                title={isAuditioningMaster ? 'Pause Master Audition' : 'Audition Master Mix'}
              >
                {isAuditioningMaster ? <Pause className="w-3.5 h-3.5 fill-current" /> : <Play className="w-3.5 h-3.5 fill-current" />}
                <span>{isAuditioningMaster ? 'Pause' : 'Audition'}</span>
              </button>
              <a
                href={masterAudioUrl}
                download="voxforge_stories_master_mix.wav"
                className="p-1.5 rounded-lg bg-emerald-500/20 text-emerald-300 border border-emerald-500/40 hover:bg-emerald-500/30 transition-colors"
                title="Download Master Mix"
              >
                <Download className="w-4 h-4" />
              </a>
            </div>
          )}
        </div>
      </div>

      {/* Clip Addition Quick Bar */}
      <div className="p-3 bg-[#0D1219] border-b border-[#242E3D] flex flex-wrap items-center gap-3">
        <span className="text-xs font-mono text-[#94A3B8] shrink-0">Insert Dialogue Line:</span>
        <select
          value={activeTrackIdForAdd}
          onChange={(e) => setActiveTrackIdForAdd(e.target.value)}
          className="bg-[#121820] border border-[#242E3D] rounded px-2.5 py-1 text-xs text-white font-mono focus:border-amber-500 focus:outline-none"
        >
          {tracks.map((t) => (
            <option key={t.id} value={t.id}>
              {t.name}
            </option>
          ))}
        </select>
        <input
          type="text"
          value={newClipText}
          onChange={(e) => setNewClipText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') handleAddClip(activeTrackIdForAdd);
          }}
          placeholder="Type dialogue line to place at current cursor position..."
          className="flex-1 min-w-[200px] bg-[#121820] border border-[#242E3D] rounded px-3 py-1 text-xs text-white placeholder-[#64748B] focus:border-amber-500 focus:outline-none font-mono"
        />
        <button
          onClick={() => handleAddClip(activeTrackIdForAdd)}
          className="px-3 py-1 rounded bg-amber-500 hover:bg-amber-400 text-black text-xs font-mono font-semibold transition-colors flex items-center space-x-1"
        >
          <Plus className="w-3 h-3" />
          <span>Add to Track</span>
        </button>
        {timelineStatus && (
          <span
            className={`text-[11px] font-mono px-2.5 py-1 rounded border ${
              timelineStatus.isError
                ? 'bg-red-500/10 text-red-300 border-red-500/30'
                : 'bg-emerald-500/10 text-emerald-300 border-emerald-500/30'
            }`}
          >
            {timelineStatus.text}
          </span>
        )}
      </div>

      {/* Main Multi-Track DAW Workspace */}
      <div className="flex-1 flex overflow-hidden">
        {/* Track Headers (Left Column) */}
        <div className="w-72 border-r border-[#242E3D] bg-[#121820] flex flex-col shrink-0 select-none">
          {/* Ruler spacer */}
          <div className="h-7 border-b border-[#242E3D] bg-[#0E131A] px-3 flex items-center text-[10px] font-mono text-[#64748B]">
            TRACK ASSIGNMENTS
          </div>

          <div className="flex-1 overflow-y-auto divide-y divide-[#242E3D]">
            {tracks.map((track) => (
              <div key={track.id} className="h-28 p-3 flex flex-col justify-between bg-[#121820]">
                <div className="flex items-center justify-between">
                  <div className="flex items-center space-x-2">
                    <span
                      className="w-2.5 h-2.5 rounded-full"
                      style={{ backgroundColor: track.color }}
                    />
                    <input
                      type="text"
                      value={track.name}
                      onChange={(e) =>
                        setTracks((prev) =>
                          prev.map((t) => (t.id === track.id ? { ...t, name: e.target.value } : t))
                        )
                      }
                      className="bg-transparent text-xs font-bold text-white focus:outline-none focus:border-b border-amber-500 max-w-[130px]"
                    />
                  </div>
                  <button
                    onClick={() => handleDeleteTrack(track.id)}
                    className="text-[#64748B] hover:text-red-400 transition-colors p-1"
                    title="Delete Track"
                  >
                    <Trash2 className="w-3 h-3" />
                  </button>
                </div>

                {/* Character Voice Dropdown */}
                <div>
                  <select
                    value={track.voiceId}
                    onChange={(e) => {
                      const nextVoice = e.target.value;
                      setTracks((prev) =>
                        prev.map((t) =>
                          t.id === track.id
                            ? {
                                ...t,
                                voiceId: nextVoice,
                                clips: t.clips.map((c) => ({
                                  ...c,
                                  voiceId: nextVoice,
                                  audioBlob: undefined,
                                  isSynthesized: false,
                                })),
                              }
                            : t
                        )
                      );
                      setIsMasterDirty(true);
                    }}
                    className="w-full bg-[#0B0E14] border border-[#242E3D] rounded px-2 py-1 text-[11px] font-mono text-[#CBD5E1] focus:border-amber-500 focus:outline-none"
                  >
                    {(usableVoices.length > 0 ? usableVoices : voices).map((v) => (
                      <option key={v.id} value={v.id}>
                        {v.name} ({v.language})
                      </option>
                    ))}
                  </select>
                </div>

                {/* Volume, Pan, Mute, Solo */}
                <div className="flex items-center justify-between gap-2 text-[10px] font-mono">
                  <div className="flex items-center space-x-1">
                    <button
                      onClick={() =>
                        setTracks((prev) =>
                          prev.map((t) =>
                            t.id === track.id ? { ...t, isMuted: !t.isMuted } : t
                          )
                        )
                      }
                      className={`px-1.5 py-0.5 rounded font-bold transition-colors ${
                        track.isMuted
                          ? 'bg-red-500 text-white'
                          : 'bg-[#1A222D] text-[#94A3B8] hover:text-white'
                      }`}
                      title="Mute"
                    >
                      M
                    </button>
                    <button
                      onClick={() =>
                        setTracks((prev) =>
                          prev.map((t) =>
                            t.id === track.id ? { ...t, isSoloed: !t.isSoloed } : t
                          )
                        )
                      }
                      className={`px-1.5 py-0.5 rounded font-bold transition-colors ${
                        track.isSoloed
                          ? 'bg-amber-500 text-black'
                          : 'bg-[#1A222D] text-[#94A3B8] hover:text-white'
                      }`}
                      title="Solo"
                    >
                      S
                    </button>
                  </div>

                  <div className="flex items-center space-x-1 text-[#64748B]">
                    <span>Vol:</span>
                    <input
                      type="range"
                      min="0"
                      max="1"
                      step="0.05"
                      value={track.volume}
                      onChange={(e) =>
                        setTracks((prev) =>
                          prev.map((t) =>
                            t.id === track.id ? { ...t, volume: parseFloat(e.target.value) } : t
                          )
                        )
                      }
                      className="w-16 accent-amber-500 cursor-pointer"
                    />
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Timeline Tracks Grid (Right Column) */}
        <div className="flex-1 flex flex-col overflow-x-auto overflow-y-auto bg-[#0B0E14] relative">
          {/* Time Ruler */}
          <div
            className="h-7 border-b border-[#242E3D] bg-[#0E131A] relative flex shrink-0 cursor-pointer"
            onClick={(e) => {
              const rect = e.currentTarget.getBoundingClientRect();
              const clickX = e.clientX - rect.left;
              setCurrentTimeSec(Math.max(0, clickX / zoomPxPerSec));
            }}
            style={{ width: `${totalDurationSec * zoomPxPerSec}px` }}
          >
            {Array.from({ length: Math.ceil(totalDurationSec / 2) }).map((_, i) => {
              const sec = i * 2;
              return (
                <div
                  key={sec}
                  className="absolute top-0 bottom-0 border-l border-[#242E3D] pl-1 text-[9px] font-mono text-[#64748B]"
                  style={{ left: `${sec * zoomPxPerSec}px` }}
                >
                  {Math.floor(sec / 60)}:{(sec % 60).toString().padStart(2, '0')}s
                </div>
              );
            })}
          </div>

          {/* Playhead Scrubber Needle */}
          <div
            className="absolute top-0 bottom-0 w-0.5 bg-amber-400 z-30 pointer-events-none transition-transform"
            style={{
              transform: `translateX(${currentTimeSec * zoomPxPerSec}px)`,
            }}
          >
            <div className="w-2.5 h-2.5 bg-amber-400 rotate-45 -ml-1 -mt-1 shadow-md shadow-amber-400/50" />
          </div>

          {/* Tracks Lanes */}
          <div
            className="flex-1 divide-y divide-[#1A222D]"
            style={{ width: `${totalDurationSec * zoomPxPerSec}px` }}
          >
            {tracks.map((track) => {
              const anySoloed = tracks.some((t) => t.isSoloed);
              const isInactive = anySoloed ? (!track.isSoloed || track.isMuted) : track.isMuted;
              return (
              <div
                key={track.id}
                className={`h-28 relative bg-[#0B0E14]/40 hover:bg-[#0E131A]/60 transition-opacity ${
                  isInactive ? 'opacity-40' : ''
                }`}
              >
                {/* Clips in Lane */}
                {track.clips.map((clip) => {
                  const clipLeft = clip.startSec * zoomPxPerSec;
                  const clipWidth = Math.max(80, clip.durationSec * zoomPxPerSec);
                  return (
                    <div
                      key={clip.id}
                      onMouseDown={(e) => {
                        e.stopPropagation();
                        setDraggedClip({
                          trackId: track.id,
                          clipId: clip.id,
                          startX: e.clientX,
                          initialStartSec: clip.startSec,
                        });
                      }}
                      className={`absolute top-2 bottom-2 rounded-lg border p-2 flex flex-col justify-between overflow-hidden shadow-md transition-shadow cursor-grab active:cursor-grabbing group select-none ${
                        draggedClip?.clipId === clip.id ? 'ring-2 ring-amber-400 z-20 shadow-lg' : ''
                      }`}
                      style={{
                        left: `${clipLeft}px`,
                        width: `${clipWidth}px`,
                        backgroundColor: `${track.color}15`,
                        borderColor: `${track.color}50`,
                      }}
                    >
                      <div className="flex items-center justify-between text-[10px] font-mono">
                        <span className="font-bold truncate text-white" style={{ color: track.color }}>
                          {clip.durationSec.toFixed(1)}s
                        </span>
                        <div className="flex items-center space-x-0.5">
                          <button
                            type="button"
                            onMouseDown={(e) => e.stopPropagation()}
                            onClick={(e) => {
                              e.stopPropagation();
                              void handleAuditionSingleClip(track.voiceId, clip);
                            }}
                            className={`p-0.5 rounded transition-colors ${
                              playingClipId === clip.id
                                ? 'text-emerald-400'
                                : 'text-[#94A3B8] hover:text-amber-300 opacity-0 group-hover:opacity-100'
                            }`}
                            title="Play Clip"
                          >
                            {playingClipId === clip.id ? (
                              <Pause className="w-3 h-3 fill-current" />
                            ) : (
                              <Play className="w-3 h-3 fill-current" />
                            )}
                          </button>
                          <button
                            type="button"
                            onMouseDown={(e) => e.stopPropagation()}
                            onClick={(e) => {
                              e.stopPropagation();
                              handleDeleteClip(track.id, clip.id);
                            }}
                            className="text-[#64748B] hover:text-red-400 opacity-0 group-hover:opacity-100 transition-opacity p-0.5"
                            title="Remove Line"
                          >
                            <Trash2 className="w-3 h-3" />
                          </button>
                        </div>
                      </div>

                      <p className="text-[11px] text-white line-clamp-2 leading-tight">
                        {clip.text}
                      </p>

                      <div className="flex items-center justify-between text-[9px] font-mono text-[#64748B]">
                        <span className="truncate">
                          {voices.find((v) => v.id === clip.voiceId)?.name || 'Voice'}
                        </span>
                        <FileAudio className="w-3 h-3 shrink-0" style={{ color: track.color }} />
                      </div>
                    </div>
                  );
                })}
              </div>
            );
          })}
          </div>
        </div>
      </div>
    </div>
  );
};
