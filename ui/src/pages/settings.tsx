import { useState } from 'react';
import { Outlet } from 'react-router';
import { fetchJson, refreshData, useTitle } from '@/api';
import { Check, FolderOpen } from '@/components/icons';
import { ConnectCard, ShareCard } from '@/components/link-client';
import { GeneralPrefs, PlayPrefs } from '@/components/prefs-client';
import { AnalysisCard, FormatSupport, RescanCard, ZoneCard } from '@/components/settings-client';
import { SettingsNav } from '@/components/settings-nav';
import { useToast } from '@/components/toast';
import { bytes, count, span } from '@/lib/time';
import { useLibraryInfo } from '@/library';

export function SettingsLayout() {
  return (
    <div className="settings">
      <SettingsNav />
      <div className="settings-body">
        <Outlet />
      </div>
    </div>
  );
}

export function GeneralSettings() {
  useTitle('설정 · 일반');
  return (
    <section>
      <h2 className="settings-h2">일반</h2>
      <p className="settings-lede">이 기기에서의 화면 취향입니다. 이 기기에 저장되며 다른 기기에는 영향을 주지 않습니다.</p>
      <GeneralPrefs />
    </section>
  );
}

export function PlaySettings() {
  useTitle('설정 · 재생 동작');
  return (
    <section>
      <h2 className="settings-h2">재생 동작</h2>
      <p className="settings-lede">누르면 무엇을 할지 정합니다.</p>
      <PlayPrefs />
    </section>
  );
}

/** 음악 폴더 바꾸기. 앱에서는 폴더 고르기 창이 뜨고, 브라우저에서는 경로를 적는다. */
function MusicDirRow({ current, canPick }: { current: string; canPick: boolean }) {
  const [value, setValue] = useState(current);
  const [busy, setBusy] = useState(false);
  const toast = useToast();

  const save = async (dir: string) => {
    setBusy(true);
    try {
      await fetchJson('/api/settings/music-dir', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ musicDir: dir }),
      });
      toast('음악 폴더를 바꿨습니다. 다시 스캔합니다.');
      refreshData();
    } catch (e) {
      toast(e instanceof Error ? e.message : '바꾸지 못했습니다', 'error');
    } finally {
      setBusy(false);
    }
  };

  // 앱이면 소리 서버가 운영체제의 폴더 고르기 창을 띄운다.
  const pick = async () => {
    setBusy(true);
    try {
      const r = await fetchJson<{ ok: boolean; musicDir?: string }>('/api/settings/pick-music-dir', { method: 'POST' });
      if (r.ok && r.musicDir) {
        setValue(r.musicDir);
        toast('음악 폴더를 바꿨습니다. 다시 스캔합니다.');
        refreshData();
      }
    } catch (e) {
      toast(e instanceof Error ? e.message : '바꾸지 못했습니다', 'error');
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="pref-row pref-row-stack">
      <div>
        <div className="pref-label">음악 폴더</div>
        <div className="pref-hint">읽기만 합니다. 바꾸면 곧바로 다시 스캔합니다.</div>
      </div>
      <form
        className="dir-form"
        onSubmit={(e) => {
          e.preventDefault();
          void save(value);
        }}
      >
        <label className="filter">
          <FolderOpen size={16} />
          <input value={value} onChange={(e) => setValue(e.target.value)} spellCheck={false} aria-label="음악 폴더 경로" />
        </label>
        {canPick ? (
          <button type="button" className="btn btn-sm" onClick={pick} disabled={busy}>
            폴더 고르기
          </button>
        ) : null}
        <button type="submit" className="btn btn-sm" disabled={busy || value.trim() === current}>
          <Check size={15} /> 바꾸기
        </button>
      </form>
    </div>
  );
}

export function LibrarySettings() {
  useTitle('설정 · 라이브러리');
  const info = useLibraryInfo();
  const s = info.summary;
  return (
    <section>
      <h2 className="settings-h2">라이브러리</h2>
      <p className="settings-lede">음악 폴더는 읽기만 합니다. 색인과 표지, 분석 결과는 캐시 폴더에 두며 지워도 다시 만들어집니다.</p>
      <MusicDirRow key={info.musicDir} current={info.musicDir} canPick={!!info.host && info.host !== 'android'} />
      <div className="pref-row">
        <div className="pref-label">캐시 폴더</div>
        <code className="pref-code">{info.dataDir}</code>
      </div>
      <div className="pref-row">
        <div className="pref-label">들어 있는 것</div>
        <div className="pref-value">
          앨범 {count(s.albums)} · 아티스트 {count(s.artists)} · 곡 {count(s.tracks)} · {span(s.duration)} · {bytes(s.bytes)}
          <br />
          <span className="muted">
            무손실 앨범 {count(s.lossless)}장 · 그중 Hi-Res {count(s.hires)}장
          </span>
        </div>
      </div>
      <RescanCard initial={info.status} scannedAt={s.scannedAt} />
      <h3 className="pref-group">오디오 분석</h3>
      <AnalysisCard initial={info.analysis} />
    </section>
  );
}

function Cap({ label, hint }: { label: string; hint: string }) {
  return (
    <div className="pref-row">
      <div>
        <div className="pref-label">{label}</div>
        <div className="pref-hint">{hint}</div>
      </div>
      <span className="ok">내장</span>
    </div>
  );
}

export function AudioSettings() {
  useTitle('설정 · 오디오');
  return (
    <section>
      <h2 className="settings-h2">오디오</h2>
      <p className="settings-lede">
        Sori에서는 곡을 트는 기기 하나하나가 존입니다. 이 컴퓨터나 휴대폰의 오디오 장치로 직접 내보내는 존과, 브라우저로 원본을 받아 트는 존이
        있습니다. DSP 설정은 존마다 따로 저장됩니다.
      </p>
      <ZoneCard />
      <h3 className="pref-group">소리 엔진</h3>
      <Cap label="디코더" hint="FLAC, ALAC, WAV, AIFF, MP3, AAC, Vorbis를 직접 풉니다. 정수 PCM은 64비트로 옮겨도 한 비트도 잃지 않습니다." />
      <Cap label="DSD → PCM" hint="DSF, DSDIFF를 120dB 데시메이션 필터로 176.4kHz PCM으로 바꿉니다." />
      <Cap label="64비트 DSP" hint="헤드룸, 볼륨 레벨링, 파라메트릭 EQ, 크로스피드, 컨볼루션, 샘플레이트 변환을 모두 f64로 계산합니다." />
      <Cap label="EBU R128 분석" hint="통합 라우드니스, 트루 피크, 라우드니스 범위. 볼륨 레벨링이 이 값을 씁니다." />
      <h3 className="pref-group">이 브라우저가 여는 형식</h3>
      <FormatSupport ffmpeg />
    </section>
  );
}

export function ConnectSettings() {
  useTitle('설정 · 연결');
  return (
    <section>
      <h2 className="settings-h2">연결</h2>
      <p className="settings-lede">
        이 기기는 혼자서도 다 됩니다. PC나 서버는 없어도 됩니다. 원할 때만 같은 와이파이의 다른 기기에 이 기기를 열거나, PC·서버의 소리에
        연결합니다.
      </p>
      <h3 className="pref-group">이 기기 열기</h3>
      <ShareCard />
      <h3 className="pref-group">다른 소리에 연결</h3>
      <ConnectCard />
    </section>
  );
}

export function AboutSettings() {
  useTitle('설정 · 정보');
  const info = useLibraryInfo();
  return (
    <section>
      <h2 className="settings-h2">정보</h2>
      <div className="pref-row">
        <div className="pref-label">버전</div>
        <div className="pref-value">
          Sori {info.version} · {info.platform}
        </div>
      </div>
      <div className="pref-row">
        <div className="pref-label">단축키</div>
        <div className="pref-value keys">
          <span>
            <kbd>Space</kbd> 재생/일시정지
          </span>
          <span>
            <kbd>←</kbd>
            <kbd>→</kbd> 10초
          </span>
          <span>
            <kbd>Shift</kbd>+<kbd>←</kbd>
            <kbd>→</kbd> 이전/다음 곡
          </span>
          <span>
            <kbd>M</kbd> 음소거
          </span>
          <span>
            <kbd>/</kbd> 검색
          </span>
          <span>
            <kbd>Q</kbd> 대기열
          </span>
          <span>
            <kbd>F</kbd> 지금 재생 중
          </span>
        </div>
      </div>
      <div className="pref-row">
        <div className="pref-label">라이브 아카이브</div>
        <div className="pref-value">
          공연 음원과 정보는{' '}
          <a href="https://archive.org/details/etree" target="_blank" rel="noreferrer">
            Internet Archive Live Music Archive
          </a>
          에서 옵니다.
        </div>
      </div>
      {info.auth && (
        <div className="pref-row">
          <div className="pref-label">로그인</div>
          <form action="/api/logout" method="post">
            <button type="submit" className="btn btn-sm">
              이 기기에서 로그아웃
            </button>
          </form>
        </div>
      )}
    </section>
  );
}
