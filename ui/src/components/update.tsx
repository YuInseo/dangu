'use client';

import { useState } from 'react';
import { readJSON, writeJSON } from '@/lib/player/storage';
import { progressText, shortBuild, startInstall, updateApi, useUpdate, type UpdateStatus } from '@/lib/update';
import { RefreshCw, X } from './icons';
import { useToast } from './toast';

function when(sec: number) {
  return sec ? new Date(sec * 1000).toLocaleDateString('ko-KR', { month: 'long', day: 'numeric', hour: '2-digit', minute: '2-digit' }) : '';
}

/** 새 버전이 있으면 아래쪽에 한 줄. "나중에"를 누르면 그 빌드는 다시 묻지 않는다. */
export function UpdateBanner() {
  const { status, setStatus } = useUpdate();
  const [dismissed, setDismissed] = useState(() => readJSON<string>('sori.update.dismissed'));
  const toast = useToast();
  if (!status?.supported || !status.available || !status.latest) return null;
  const busy = status.download.state === 'downloading' || status.download.state === 'installing';
  if (dismissed === status.latest.build && !busy) return null;
  const progress = progressText(status.download);
  return (
    <div className="update-bar" role="status">
      <RefreshCw size={17} className={busy ? 'spin' : undefined} />
      <div className="update-bar-text">
        <strong>새 버전이 있습니다</strong>
        <span className="update-bar-sub">{progress ?? (status.latest.notes || `빌드 ${shortBuild(status.latest.build)}`)}</span>
      </div>
      <button
        type="button"
        className="btn btn-primary btn-sm"
        disabled={busy}
        onClick={() => startInstall(setStatus).catch((e: unknown) => toast(e instanceof Error ? e.message : '업데이트하지 못했습니다', 'error'))}
      >
        업데이트
      </button>
      {!busy && (
        <button
          type="button"
          className="icon-btn"
          aria-label="나중에"
          title="나중에"
          onClick={() => {
            writeJSON('sori.update.dismissed', status.latest!.build);
            setDismissed(status.latest!.build);
          }}
        >
          <X size={17} />
        </button>
      )}
    </div>
  );
}

/** 설정 · 정보: 이 빌드, 새 버전, 지금 확인, 자동으로 받아 두기 */
export function UpdateCard() {
  const { status, setStatus } = useUpdate(60_000);
  const [checking, setChecking] = useState(false);
  const toast = useToast();
  if (!status) return null;
  const busy = status.download.state === 'downloading' || status.download.state === 'installing';
  const progress = progressText(status.download);

  const check = async () => {
    setChecking(true);
    try {
      const s = await updateApi.check();
      setStatus(s);
      if (!s.available) toast('최신 버전입니다');
    } catch (e) {
      toast(e instanceof Error ? e.message : '확인하지 못했습니다', 'error');
    } finally {
      setChecking(false);
    }
  };

  return (
    <>
      <div className="pref-row">
        <div className="pref-label">빌드</div>
        <div className="pref-value">
          {status.build ? `${shortBuild(status.build)} · ${when(status.buildTime)}` : '직접 빌드한 것(자동 업데이트 없음)'}
        </div>
      </div>
      {status.supported && (
        <div className="pref-row pref-row-stack">
          <div className="pref-label">업데이트</div>
          <div className="pref-value">
            <UpdateLine status={status} progress={progress} />
            <div className="log-actions">
              {status.available && (
                <button
                  type="button"
                  className="btn btn-primary btn-sm"
                  disabled={busy}
                  onClick={() => startInstall(setStatus).catch((e: unknown) => toast(e instanceof Error ? e.message : '업데이트하지 못했습니다', 'error'))}
                >
                  업데이트
                </button>
              )}
              <button type="button" className="btn btn-sm" onClick={() => void check()} disabled={checking || busy}>
                <RefreshCw size={15} className={checking ? 'spin' : undefined} /> 지금 확인
              </button>
            </div>
            <label className="update-auto">
              <input type="checkbox" checked={status.auto} onChange={(e) => updateApi.setAuto(e.target.checked).then(setStatus)} />
              새 버전을 자동으로 확인하고 미리 받아 두기
            </label>
          </div>
        </div>
      )}
    </>
  );
}

function UpdateLine({ status, progress }: { status: UpdateStatus; progress: string | null }) {
  if (progress) return <div className={status.download.state === 'failed' ? 'warn' : undefined}>{progress}</div>;
  if (status.available && status.latest) {
    return (
      <div>
        새 버전 {shortBuild(status.latest.build)} · {when(status.latest.time)}
        {status.latest.notes && <div className="muted">{status.latest.notes}</div>}
        {status.download.state === 'ready' && <div className="muted">받아 두었습니다. 누르면 바로 설치합니다.</div>}
      </div>
    );
  }
  if (status.error) return <div className="warn">{status.error}</div>;
  return <div className="muted">{status.checkedAt ? `최신 버전입니다 · ${new Date(status.checkedAt).toLocaleTimeString('ko-KR')} 확인` : '아직 확인하지 않았습니다'}</div>;
}
