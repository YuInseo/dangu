'use client';

import Link from '@/compat/link';
import { useEffect, useState } from 'react';
import type { AnalysisStatus, ScanStatus } from '@/lib/library/types';
import { usePlayer, useZone, type ZoneKind } from '@/lib/player/context';
import { NativeEngine, type OutputInfo } from '@/lib/player/native';
import { canPlayNatively } from '@/lib/player/engine';
import { khz } from '@/lib/quality';
import { Check, CircleAlert, LoaderCircle, Monitor, RefreshCw, Speaker, X } from './icons';
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

/** 백그라운드 오디오 분석 속도와 진행. 룬의 "Background audio analysis speed". */
export function AnalysisCard({ initial }: { initial: AnalysisStatus }) {
  const [s, setS] = useState(initial);
  useEffect(() => {
    let alive = true;
    const t = setInterval(async () => {
      try {
        const r = await fetch('/api/analysis', { cache: 'no-store' });
        if (r.ok && alive) setS((await r.json()) as AnalysisStatus);
      } catch {
        // 다음에
      }
    }, 3000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, []);
  const set = async (speed: AnalysisStatus['speed']) => {
    setS((x) => ({ ...x, speed }));
    const r = await fetch('/api/analysis', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ speed }),
    });
    if (r.ok) setS((await r.json()) as AnalysisStatus);
  };
  const pct = s.total ? Math.round((s.done / s.total) * 100) : 0;
  return (
    <div className="analysis">
      <div className="pref-row">
        <div>
          <div className="pref-label">백그라운드 오디오 분석 속도</div>
          <div className="pref-hint">
            곡마다 EBU R128 라우드니스·트루 피크·파형을 구합니다. 볼륨 레벨링과 파형 진행 막대가 이 값을 씁니다. 느리게는 한 곡씩 낮은
            우선순위로, 빠르게는 CPU 코어 절반까지 씁니다.
          </div>
        </div>
        <div className="pref-control">
          <div className="seg seg-sm" role="radiogroup" aria-label="분석 속도">
            {(
              [
                ['off', '끔'],
                ['throttled', '느리게'],
                ['fast', '빠르게'],
              ] as const
            ).map(([v, label]) => (
              <button
                key={v}
                type="button"
                role="radio"
                aria-checked={s.speed === v}
                className={`seg-item${s.speed === v ? ' is-active' : ''}`}
                onClick={() => set(v)}
              >
                {label}
              </button>
            ))}
          </div>
        </div>
      </div>
      <div className="analysis-progress">
        <div className="bar">
          <span style={{ width: `${pct}%` }} />
        </div>
        <span className="muted">
          {s.done.toLocaleString('ko-KR')} / {s.total.toLocaleString('ko-KR')}곡 분석됨
          {s.state === 'running' ? ' · 분석 중' : s.state === 'off' ? ' · 꺼짐' : s.done >= s.total ? ' · 모두 끝남' : ''}
        </span>
      </div>
    </div>
  );
}

/** 이 브라우저 = 하나의 존. 룬의 "Audio → 장치" 화면에 해당한다. */
export function ZoneCard() {
  const { engine, setZone } = usePlayer();
  const { zone, nativeAvailable } = useZone();
  const [rate, setRate] = useState<number | null>(null);
  const [browser, setBrowser] = useState('브라우저');
  const [native, setNative] = useState<OutputInfo | null>(null);
  useEffect(() => {
    const ua = navigator.userAgent;
    setBrowser(
      /Edg\//.test(ua) ? 'Edge' : /Firefox\//.test(ua) ? 'Firefox' : /Chrome\//.test(ua) ? 'Chrome' : /Safari\//.test(ua) ? 'Safari' : '브라우저',
    );
    try {
      const ctx = new AudioContext();
      setRate(ctx.sampleRate);
      void ctx.close();
    } catch {
      // 모름
    }
  }, []);
  useEffect(() => {
    if (engine instanceof NativeEngine) {
      const read = () => setNative(engine.output);
      read();
      const off = engine.subscribe(read);
      return () => {
        off();
      };
    }
  }, [engine]);

  const rows: { key: ZoneKind; name: string; detail: string; Icon: typeof Monitor }[] = [];
  if (nativeAvailable) {
    rows.push({
      key: 'native',
      name: '이 기기',
      detail: native
        ? `${native.device} · ${native.api} · ${khz(native.rate)}kHz · 소리 엔진이 64비트로 직접`
        : '소리 엔진이 이 기기의 오디오 장치로 직접 냅니다 · 64비트',
      Icon: Speaker,
    });
  }
  rows.push({
    key: 'browser',
    name: '이 브라우저',
    detail: `${browser} · 기본 출력 장치${rate ? ` ${khz(rate)}kHz` : ''} · OS 믹서(공유 모드)`,
    Icon: Monitor,
  });

  return (
    <div className="zone-list">
      {rows.map(({ key, name, detail, Icon }) => (
        <div key={key} className={`zone-card${zone === key ? ' is-active' : ''}`}>
          <div className="zone-card-icon">
            <Icon size={26} />
          </div>
          <div className="zone-card-text">
            <div className="zone-card-name">
              {name}
              {zone === key && <span className="zone-card-now">지금 여기서</span>}
            </div>
            <div className="muted">{detail}</div>
          </div>
          {zone === key ? (
            <Link href="/dsp" className="btn btn-sm">
              DSP 설정
            </Link>
          ) : (
            <button type="button" className="btn btn-sm" onClick={() => setZone(key)}>
              여기서 틀기
            </button>
          )}
        </div>
      ))}
    </div>
  );
}
