'use client';

import { useEffect, useState } from 'react';
import type { ScanStatus } from '@/lib/library/types';
import { canPlayNatively } from '@/lib/player/engine';
import { khz } from '@/lib/quality';
import { Check, CircleAlert, LoaderCircle, RefreshCw, X } from './icons';
import { scanText, useScanStatus } from './scan-status';

export function RescanCard({ initial, scannedAt }: { initial: ScanStatus; scannedAt?: number }) {
  const { status, rescan } = useScanStatus(initial);
  const last = status.finishedAt ?? scannedAt;
  return (
    <div className="rescan">
      <div>
        {status.state === 'scanning' ? (
          <span className="muted">
            <LoaderCircle size={14} className="spin" /> {scanText(status)}
          </span>
        ) : status.state === 'error' ? (
          <span className="warn">
            <CircleAlert size={14} /> {status.error}
          </span>
        ) : (
          <span className="muted">
            {last ? `마지막 스캔 ${new Date(last).toLocaleString('ko-KR')}` : '아직 스캔하지 않았습니다'}
            {status.added || status.removed ? ` · 새로 ${status.added ?? 0}곡, 빠짐 ${status.removed ?? 0}곡` : ''}
          </span>
        )}
      </div>
      <button type="button" className="btn btn-sm" onClick={rescan} disabled={status.state === 'scanning'}>
        <RefreshCw size={15} /> 다시 스캔
      </button>
    </div>
  );
}

export function OutputInfo() {
  const [rate, setRate] = useState<number | null>(null);
  useEffect(() => {
    try {
      const ctx = new AudioContext();
      setRate(ctx.sampleRate);
      void ctx.close();
    } catch {
      // 모름
    }
  }, []);
  return (
    <dl className="kv">
      <dt>출력 장치</dt>
      <dd>
        {rate ? (
          <>
            기본 출력 장치가 <strong>{khz(rate)}kHz</strong>로 열려 있습니다. 이와 다른 샘플레이트의 곡은 브라우저가 리샘플링하고, 신호
            경로에 고품질로 표시됩니다. 비트퍼펙트로 들으려면 운영체제의 출력 샘플레이트를 곡에 맞추세요(macOS: 오디오 MIDI 설정, Windows:
            소리 → 장치 속성 → 고급).
          </>
        ) : (
          '알 수 없음'
        )}
      </dd>
    </dl>
  );
}

const FORMATS = [
  { name: 'FLAC', mime: 'audio/flac', lossless: true },
  { name: 'ALAC', mime: 'audio/mp4; codecs="alac"', lossless: true },
  { name: 'WAV', mime: 'audio/wav', lossless: true },
  { name: 'AIFF', mime: 'audio/aiff', lossless: true },
  { name: 'DSD (DSF/DFF)', mime: 'audio/x-dsf', lossless: true },
  { name: 'WavPack', mime: 'audio/x-wavpack', lossless: true },
  { name: 'APE', mime: 'audio/x-ape', lossless: true },
  { name: 'MP3', mime: 'audio/mpeg', lossless: false },
  { name: 'AAC', mime: 'audio/mp4; codecs="mp4a.40.2"', lossless: false },
  { name: 'Ogg Vorbis', mime: 'audio/ogg; codecs="vorbis"', lossless: false },
  { name: 'Opus', mime: 'audio/ogg; codecs="opus"', lossless: false },
];

export function FormatSupport({ ffmpeg }: { ffmpeg: boolean }) {
  const [support, setSupport] = useState<Record<string, boolean> | null>(null);
  useEffect(() => {
    setSupport(Object.fromEntries(FORMATS.map((f) => [f.name, canPlayNatively(f.mime)])));
  }, []);
  return (
    <div className="formats">
      {FORMATS.map((f) => {
        const native = support?.[f.name];
        const how = native == null ? '…' : native ? '원본 그대로' : f.lossless && ffmpeg ? 'FLAC으로 변환' : ffmpeg ? '변환' : '재생 불가';
        const cls = native ? 'ok' : ffmpeg ? 'mid' : 'no';
        return (
          <div key={f.name} className={`format format-${native == null ? 'wait' : cls}`}>
            <span className="format-icon">{native ? <Check size={14} /> : ffmpeg ? <RefreshCw size={13} /> : <X size={14} />}</span>
            <span className="format-name">{f.name}</span>
            <span className="format-how">{how}</span>
          </div>
        );
      })}
    </div>
  );
}
