'use client';

import { useState } from 'react';
import { isLocalOrigin } from '@/lib/link';
import { readJSON, writeJSON } from '@/lib/player/storage';
import {
  buildName,
  progressText,
  shortBuild,
  startInstall,
  updateApi,
  upToDateText,
  useUpdate,
  whenText as when,
  type UpdateStatus,
} from '@/lib/update';
import { RefreshCw, X } from './icons';
import { useToast } from './toast';

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
        <span className="update-bar-sub">
          {progress ?? [buildName(status.latest.number, status.latest.build), status.latest.notes].filter(Boolean).join(' · ')}
        </span>
      </div>
      <button
        type="button"
        className="btn btn-primary btn-sm"
        disabled={busy}
        onClick={() =>
          startInstall(setStatus).catch((e: unknown) => toast(e instanceof Error ? e.message : '업데이트하지 못했습니다', 'error'))
        }
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
      if (s.error) toast(s.error, 'error');
      else if (!s.available) toast(upToDateText(s));
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
          {status.build ? (
            <>
              {buildName(status.number, status.build)} · {when(status.buildTime)}
              {status.number ? <span className="muted"> ({shortBuild(status.build)})</span> : null}
            </>
          ) : (
            '직접 빌드한 것(자동 업데이트 없음)'
          )}
          {/* 다른 기기(PC·서버)의 소리에 이어 보고 있으면, 여기 나오는 버전은 그 기기의 것이다 */}
          {!isLocalOrigin() && (
            <div className="muted">지금 보는 화면은 연결한 다른 기기의 소리입니다. 빌드와 업데이트도 그 기기의 것입니다.</div>
          )}
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
                  onClick={() =>
                    startInstall(setStatus).catch((e: unknown) =>
                      toast(e instanceof Error ? e.message : '업데이트하지 못했습니다', 'error'),
                    )
                  }
                >
                  업데이트
                </button>
              )}
              <button type="button" className="btn btn-sm" onClick={() => void check()} disabled={checking || busy}>
                <RefreshCw size={15} className={checking ? 'spin' : undefined} /> 지금 확인
              </button>
            </div>
            <label className="update-auto">
              <input type="checkbox" checked={status.auto} onChange={(e) => updateApi.setAuto(e.target.checked).then(setStatus)} />새 버전을
              자동으로 확인하고 미리 받아 두기
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
        새 버전 {buildName(status.latest.number, status.latest.build)} · {when(status.latest.time)}
        {status.latest.notes && <div className="muted">{status.latest.notes}</div>}
        {status.download.state === 'ready' && <div className="muted">받아 두었습니다. 누르면 바로 설치합니다.</div>}
      </div>
    );
  }
  if (status.error) return <div className="warn">{status.error}</div>;
  return (
    <div className="muted">
      {status.checkedAt ? `최신 버전입니다 · ${new Date(status.checkedAt).toLocaleTimeString('ko-KR')} 확인` : '아직 확인하지 않았습니다'}
      {/* 지금 깔린 것이 서버의 가장 새 빌드와 같은지 눈으로 보이게 */}
      {status.latest &&
        (status.latest.build === status.build ? (
          <div>지금 쓰는 것이 올라온 가장 새 빌드입니다.</div>
        ) : (
          <div>
            올라온 가장 새 빌드: {buildName(status.latest.number, status.latest.build)} · {when(status.latest.time)}
          </div>
        ))}
    </div>
  );
}
